use core::str;

use alephium_crypto::{self, PublicKey, Signature};
use alephium_serde::{DecodeResult, Error, Staging, bounded, compact, fixed};

pub const HANDSHAKE_MAX_CLIENT_ID_BYTES: usize = 256;
const HASH_BYTES: usize = alephium_crypto::HASH_LENGTH;
const MAX_REQUEST_VECTOR_ITEMS: usize = 1_048_576;
const MAX_WEIGHT_BYTES: usize = 8_192;
const MAX_TX_VECTOR_ITEMS: usize = 1_048_576;

// Official payload code table. Codes are positional in the official `Payload`
// serde, so the whole table stays even though the fast path decodes only the
// handshake, liveness, inventory, chain-state, and transaction payloads.
pub const HELLO_CODE: i32 = 0;
pub const PING_CODE: i32 = 1;
pub const PONG_CODE: i32 = 2;
pub const BLOCKS_REQUEST_CODE: i32 = 3;
pub const BLOCKS_RESPONSE_CODE: i32 = 4;
pub const HEADERS_REQUEST_CODE: i32 = 5;
pub const HEADERS_RESPONSE_CODE: i32 = 6;
pub const INV_REQUEST_CODE: i32 = 7;
pub const INV_RESPONSE_CODE: i32 = 8;
pub const NEW_BLOCK_CODE: i32 = 9;
pub const NEW_HEADER_CODE: i32 = 10;
pub const NEW_INV_CODE: i32 = 11;
pub const NEW_BLOCK_HASH_CODE: i32 = 12;
pub const NEW_TX_HASHES_CODE: i32 = 13;
pub const TXS_REQUEST_CODE: i32 = 14;
pub const TXS_RESPONSE_CODE: i32 = 15;
pub const CHAIN_STATE_CODE: i32 = 16;
pub const HEADERS_BY_HEIGHTS_REQUEST_CODE: i32 = 17;
pub const HEADERS_BY_HEIGHTS_RESPONSE_CODE: i32 = 18;
pub const BLOCKS_AND_UNCLES_BY_HEIGHTS_REQUEST_CODE: i32 = 19;
pub const BLOCKS_AND_UNCLES_BY_HEIGHTS_RESPONSE_CODE: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestId {
    pub value: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    pub client_id: String,
    pub timestamp: i64,
    pub broker_info: InterBrokerInfo,
    pub signature: [u8; alephium_crypto::SIGNATURE_LENGTH],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterBrokerInfo {
    pub clique_id: CliqueId,
    pub broker_id: i32,
    pub broker_num: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliqueId {
    pub bytes: [u8; alephium_crypto::PUBLIC_KEY_LENGTH],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ping {
    pub id: RequestId,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pong {
    pub id: RequestId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockHash {
    pub bytes: [u8; HASH_BYTES],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionId {
    pub bytes: [u8; HASH_BYTES],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainIndex {
    pub from: i32,
    pub to: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainTip {
    pub hash: BlockHash,
    pub height: i32,
    pub weight: Vec<u8>,
}

impl ChainTip {
    pub fn validate(&self) -> Result<(), Error> {
        if self.height >= 0 {
            Ok(())
        } else {
            Err(Error::validation(format!(
                "invalid ChainTip height: {}",
                self.height
            )))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvRequest {
    pub id: RequestId,
    pub locators: Vec<Vec<BlockHash>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvResponse {
    pub id: RequestId,
    pub hashes: Vec<Vec<BlockHash>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTxHashes {
    pub hashes: Vec<(ChainIndex, Vec<TransactionId>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxsRequest {
    pub id: RequestId,
    pub hashes: Vec<(ChainIndex, Vec<TransactionId>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxsResponse {
    pub id: RequestId,
    pub txs: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainState {
    pub tips: Vec<ChainTip>,
}

pub fn encode_hello_payload(hello: &Hello) -> Result<Vec<u8>, Error> {
    let mut encoded =
        bounded::encode_bytes(hello.client_id.as_bytes(), HANDSHAKE_MAX_CLIENT_ID_BYTES)?;
    encoded.extend_from_slice(&fixed::encode_i64_be(hello.timestamp));
    encoded.extend_from_slice(&encode_inter_broker_info_body(&hello.broker_info)?);
    encoded.extend_from_slice(&hello.signature);
    Ok(encoded)
}

pub fn decode_hello_payload(input: &[u8], configured_groups: u32) -> Result<Hello, Error> {
    let hello = decode_hello_payload_with_rest(input, configured_groups)?;
    if !hello.rest.is_empty() {
        Err(Error::redundant_bytes(
            input.len() - hello.rest.len(),
            input.len(),
        ))
    } else {
        Ok(hello.value)
    }
}

fn decode_hello_payload_with_rest(input: &[u8], configured_groups: u32) -> DecodeResult<'_, Hello> {
    let client_id = decode_handshake_client_id(input)?;
    let staged_ts = fixed::decode_i64_be(client_id.rest)?;
    let broker_info = decode_inter_broker_info(staged_ts.rest)?;
    let signature = split_signature(broker_info.rest)?;
    let hello = Hello {
        client_id: client_id.value,
        timestamp: staged_ts.value,
        broker_info: broker_info.value,
        signature: signature.value,
    };

    hello.validate(configured_groups)?;
    Ok(Staging::new(hello, signature.rest))
}

fn decode_handshake_client_id(input: &[u8]) -> DecodeResult<'_, String> {
    let staged_len = bounded::decode_bytes(input, HANDSHAKE_MAX_CLIENT_ID_BYTES)?;
    let client_id = str::from_utf8(staged_len.value)
        .map_err(|_| Error::validation("client id must be UTF-8"))?
        .to_owned();

    Ok(staged_len.map(|_| client_id))
}

fn split_signature(input: &[u8]) -> DecodeResult<'_, [u8; alephium_crypto::SIGNATURE_LENGTH]> {
    let staged = fixed::take(input, alephium_crypto::SIGNATURE_LENGTH)?;
    let mut signature = [0u8; alephium_crypto::SIGNATURE_LENGTH];
    signature.copy_from_slice(staged.value);
    Ok(staged.map(|_| signature))
}

impl Hello {
    fn validate(&self, configured_groups: u32) -> Result<(), Error> {
        self.broker_info.validate(configured_groups)?;
        if self.timestamp <= 0 {
            return Err(Error::validation("Invalid Hello timestamp"));
        }

        let signature = Signature::try_from(self.signature.as_slice())
            .map_err(|_| Error::validation("Invalid Hello signature"))?;
        let clique_id = PublicKey::try_from(self.broker_info.clique_id.bytes.as_slice())
            .map_err(|_| Error::validation("Invalid Hello signature"))?;

        if !alephium_crypto::verify(&self.broker_info.hash(), &signature, &clique_id) {
            return Err(Error::validation("Invalid Hello signature"));
        }

        Ok(())
    }
}

pub fn encode_ping_payload(ping: &Ping) -> Vec<u8> {
    let mut encoded = compact::encode_u32(ping.id.value);
    encoded.extend_from_slice(&fixed::encode_i64_be(ping.timestamp));
    encoded
}

pub fn decode_ping_payload(input: &[u8]) -> DecodeResult<'_, Ping> {
    let staged_id = compact::decode_u32(input)?;
    let staged_timestamp = fixed::decode_i64_be(staged_id.rest)?;
    Ok(staged_timestamp.map(|timestamp| Ping {
        id: RequestId {
            value: staged_id.value,
        },
        timestamp,
    }))
}

pub fn encode_pong_payload(pong: &Pong) -> Vec<u8> {
    compact::encode_u32(pong.id.value)
}

pub fn decode_pong_payload(input: &[u8]) -> DecodeResult<'_, Pong> {
    let staged = compact::decode_u32(input)?;
    Ok(staged.map(|value| Pong {
        id: RequestId { value },
    }))
}

pub fn encode_inv_request_payload(request: &InvRequest) -> Result<Vec<u8>, Error> {
    let mut encoded = encode_request_id(request.id);
    encoded.extend_from_slice(&encode_nested_block_hash_vector(&request.locators)?);
    Ok(encoded)
}

pub fn decode_inv_request_payload(input: &[u8]) -> DecodeResult<'_, InvRequest> {
    let id = decode_request_id(input)?;
    let locators = decode_nested_block_hash_vector(id.rest)?;
    Ok(locators.map(|locators| InvRequest {
        id: RequestId { value: id.value },
        locators,
    }))
}

pub fn encode_inv_response_payload(response: &InvResponse) -> Result<Vec<u8>, Error> {
    let mut encoded = encode_request_id(response.id);
    encoded.extend_from_slice(&encode_nested_block_hash_vector(&response.hashes)?);
    Ok(encoded)
}

pub fn decode_inv_response_payload(input: &[u8]) -> DecodeResult<'_, InvResponse> {
    let id = decode_request_id(input)?;
    let hashes = decode_nested_block_hash_vector(id.rest)?;
    Ok(hashes.map(|hashes| InvResponse {
        id: RequestId { value: id.value },
        hashes,
    }))
}

pub fn decode_new_txhashes_payload(input: &[u8]) -> DecodeResult<'_, NewTxHashes> {
    let hashes = decode_chain_indexed_txid_vector(input)?;
    Ok(hashes.map(|hashes| NewTxHashes { hashes }))
}

pub fn encode_txs_request_payload(request: &TxsRequest) -> Result<Vec<u8>, Error> {
    let mut encoded = encode_request_id(request.id);
    encoded.extend_from_slice(&encode_chain_indexed_txid_vector(&request.hashes)?);
    Ok(encoded)
}

pub fn decode_txs_request_payload(input: &[u8]) -> DecodeResult<'_, TxsRequest> {
    let id = decode_request_id(input)?;
    let hashes = decode_chain_indexed_txid_vector(id.rest)?;
    Ok(hashes.map(|hashes| TxsRequest {
        id: RequestId { value: id.value },
        hashes,
    }))
}

pub fn encode_txs_response_payload(response: &TxsResponse) -> Result<Vec<u8>, Error> {
    let mut encoded = encode_request_id(response.id);
    encoded.extend_from_slice(&encode_raw_byte_vector(&response.txs)?);
    Ok(encoded)
}

pub fn decode_chain_state_payload(input: &[u8]) -> DecodeResult<'_, ChainState> {
    let tips = decode_chain_tips(input)?;
    Ok(tips.map(|tips| ChainState { tips }))
}

fn encode_request_id(id: RequestId) -> Vec<u8> {
    compact::encode_u32(id.value)
}

fn decode_request_id(input: &[u8]) -> DecodeResult<'_, u32> {
    compact::decode_u32(input)
}

fn encode_hash(hash: &BlockHash) -> Vec<u8> {
    hash.bytes.to_vec()
}

fn decode_hash(input: &[u8]) -> DecodeResult<'_, BlockHash> {
    fixed::decode_array::<HASH_BYTES>(input).map(|value| value.map(|bytes| BlockHash { bytes }))
}

fn encode_tx_id(tx_id: &TransactionId) -> Vec<u8> {
    tx_id.bytes.to_vec()
}

fn decode_tx_id(input: &[u8]) -> DecodeResult<'_, TransactionId> {
    fixed::decode_array::<HASH_BYTES>(input).map(|value| value.map(|bytes| TransactionId { bytes }))
}

fn encode_chain_index(index: &ChainIndex) -> Result<Vec<u8>, Error> {
    if index.from < 0 || index.to < 0 {
        return Err(Error::validation("invalid ChainIndex values"));
    }
    let mut encoded = compact::encode_i32(index.from);
    encoded.extend_from_slice(&compact::encode_i32(index.to));
    Ok(encoded)
}

fn decode_chain_index(input: &[u8]) -> DecodeResult<'_, ChainIndex> {
    let from = compact::decode_i32(input)?;
    let to = compact::decode_i32(from.rest)?;
    if from.value < 0 || to.value < 0 {
        Err(Error::validation(format!(
            "invalid ChainIndex values {}->{}",
            from.value, to.value
        )))
    } else {
        Ok(to.map(|to| ChainIndex {
            from: from.value,
            to,
        }))
    }
}

fn decode_weight(input: &[u8]) -> DecodeResult<'_, Vec<u8>> {
    let staged_len = compact::decode_i32(input)?;
    if staged_len.value < 0 {
        return Err(Error::validation(format!(
            "invalid weight length: {}",
            staged_len.value
        )));
    }
    let len = usize::try_from(staged_len.value).map_err(|_| {
        Error::validation(format!(
            "invalid weight length conversion: {}",
            staged_len.value
        ))
    })?;
    if len > MAX_WEIGHT_BYTES {
        return Err(Error::LimitExceeded {
            declared: len,
            limit: MAX_WEIGHT_BYTES,
        });
    }
    let staged = fixed::take(staged_len.rest, len)?;
    Ok(staged.map(|weight| weight.to_vec()))
}

fn decode_chain_tip(input: &[u8]) -> DecodeResult<'_, ChainTip> {
    let hash = decode_hash(input)?;
    // `Serde[Int]`: compact signed int, as in `ChainTip.serde`.
    let height = compact::decode_i32(hash.rest)?;
    let weight = decode_weight(height.rest)?;
    let tip = ChainTip {
        hash: hash.value,
        height: height.value,
        weight: weight.value.clone(),
    };
    tip.validate()?;
    Ok(Staging::new(tip, weight.rest))
}

fn decode_chain_tips(input: &[u8]) -> DecodeResult<'_, Vec<ChainTip>> {
    let staged = compact::decode_i32(input)?;
    if staged.value < 0 {
        return Err(Error::validation(format!(
            "negative chain tip count: {}",
            staged.value
        )));
    }
    let len = usize::try_from(staged.value).map_err(|_| {
        Error::validation(format!(
            "invalid chain tip count conversion: {}",
            staged.value
        ))
    })?;
    if len > MAX_REQUEST_VECTOR_ITEMS {
        return Err(Error::LimitExceeded {
            declared: len,
            limit: MAX_REQUEST_VECTOR_ITEMS,
        });
    }
    let mut rest = staged.rest;
    let mut tips = Vec::with_capacity(len);
    for _ in 0..len {
        let staged_tip = decode_chain_tip(rest)?;
        tips.push(staged_tip.value);
        rest = staged_tip.rest;
    }
    Ok(Staging::new(tips, rest))
}

fn encode_block_hash_vector(values: &[BlockHash]) -> Result<Vec<u8>, Error> {
    bounded::encode_vec(values, MAX_REQUEST_VECTOR_ITEMS, |hash, encoded| {
        encoded.extend_from_slice(&encode_hash(hash));
        Ok(())
    })
}

fn decode_block_hash_vector(input: &[u8]) -> DecodeResult<'_, Vec<BlockHash>> {
    let staged_len = compact::decode_i32(input)?;
    if staged_len.value < 0 {
        return Err(Error::validation(format!(
            "negative block hash vector length: {}",
            staged_len.value
        )));
    }
    let len = usize::try_from(staged_len.value).map_err(|_| {
        Error::validation(format!(
            "invalid block hash vector length: {}",
            staged_len.value
        ))
    })?;
    if len > MAX_REQUEST_VECTOR_ITEMS {
        return Err(Error::LimitExceeded {
            declared: len,
            limit: MAX_REQUEST_VECTOR_ITEMS,
        });
    }
    let mut rest = staged_len.rest;
    let mut values = Vec::with_capacity(len);
    for _ in 0..len {
        let staged = decode_hash(rest)?;
        values.push(staged.value);
        rest = staged.rest;
    }
    Ok(Staging::new(values, rest))
}

fn encode_nested_block_hash_vector(values: &[Vec<BlockHash>]) -> Result<Vec<u8>, Error> {
    bounded::encode_vec(values, MAX_REQUEST_VECTOR_ITEMS, |row, encoded| {
        encoded.extend_from_slice(&encode_block_hash_vector(row)?);
        Ok(())
    })
}

fn decode_nested_block_hash_vector(input: &[u8]) -> DecodeResult<'_, Vec<Vec<BlockHash>>> {
    let staged_len = compact::decode_i32(input)?;
    if staged_len.value < 0 {
        return Err(Error::validation(format!(
            "negative chain count: {}",
            staged_len.value
        )));
    }
    let len = usize::try_from(staged_len.value).map_err(|_| {
        Error::validation(format!(
            "invalid chain count conversion: {}",
            staged_len.value
        ))
    })?;
    if len > MAX_REQUEST_VECTOR_ITEMS {
        return Err(Error::LimitExceeded {
            declared: len,
            limit: MAX_REQUEST_VECTOR_ITEMS,
        });
    }

    let mut rest = staged_len.rest;
    let mut chains = Vec::with_capacity(len);
    for _ in 0..len {
        let chain = decode_block_hash_vector(rest)?;
        chains.push(chain.value);
        rest = chain.rest;
    }
    Ok(Staging::new(chains, rest))
}

fn encode_raw_byte_vector(values: &[Vec<u8>]) -> Result<Vec<u8>, Error> {
    let mut encoded =
        compact::encode_i32(
            i32::try_from(values.len()).map_err(|_| Error::LimitExceeded {
                declared: values.len(),
                limit: MAX_REQUEST_VECTOR_ITEMS,
            })?,
        );
    for value in values {
        if value.len() > MAX_TX_VECTOR_ITEMS {
            return Err(Error::LimitExceeded {
                declared: value.len(),
                limit: MAX_TX_VECTOR_ITEMS,
            });
        }
        encoded.extend_from_slice(value);
    }
    Ok(encoded)
}

fn encode_chain_indexed_txid_vector(
    values: &[(ChainIndex, Vec<TransactionId>)],
) -> Result<Vec<u8>, Error> {
    bounded::encode_vec(values, MAX_REQUEST_VECTOR_ITEMS, |pair, encoded| {
        encoded.extend_from_slice(&encode_chain_index(&pair.0)?);
        encoded.extend_from_slice(&encode_txids(pair.1.as_slice())?);
        Ok(())
    })
}

fn decode_chain_indexed_txid_vector(
    input: &[u8],
) -> DecodeResult<'_, Vec<(ChainIndex, Vec<TransactionId>)>> {
    let staged_len = compact::decode_i32(input)?;
    if staged_len.value < 0 {
        return Err(Error::validation(format!(
            "negative chain index bucket count: {}",
            staged_len.value
        )));
    }
    let len = usize::try_from(staged_len.value).map_err(|_| {
        Error::validation(format!(
            "invalid chain index bucket count conversion: {}",
            staged_len.value
        ))
    })?;
    if len > MAX_REQUEST_VECTOR_ITEMS {
        return Err(Error::LimitExceeded {
            declared: len,
            limit: MAX_REQUEST_VECTOR_ITEMS,
        });
    }
    let mut rest = staged_len.rest;
    let mut values = Vec::with_capacity(len);
    for _ in 0..len {
        let index = decode_chain_index(rest)?;
        let txids = decode_txids(index.rest)?;
        values.push((index.value, txids.value));
        rest = txids.rest;
    }
    Ok(Staging::new(values, rest))
}

fn encode_txids(values: &[TransactionId]) -> Result<Vec<u8>, Error> {
    bounded::encode_vec(values, MAX_TX_VECTOR_ITEMS, |tx_id, encoded| {
        encoded.extend_from_slice(&encode_tx_id(tx_id));
        Ok(())
    })
}

fn decode_txids(input: &[u8]) -> DecodeResult<'_, Vec<TransactionId>> {
    let staged_len = compact::decode_i32(input)?;
    if staged_len.value < 0 {
        return Err(Error::validation(format!(
            "negative txid count: {}",
            staged_len.value
        )));
    }
    let len = usize::try_from(staged_len.value).map_err(|_| {
        Error::validation(format!(
            "invalid txid count conversion: {}",
            staged_len.value
        ))
    })?;
    if len > MAX_TX_VECTOR_ITEMS {
        return Err(Error::LimitExceeded {
            declared: len,
            limit: MAX_TX_VECTOR_ITEMS,
        });
    }
    let mut rest = staged_len.rest;
    let mut values = Vec::with_capacity(len);
    for _ in 0..len {
        let tx = decode_tx_id(rest)?;
        values.push(tx.value);
        rest = tx.rest;
    }
    Ok(Staging::new(values, rest))
}

fn encode_inter_broker_info_body(info: &InterBrokerInfo) -> Result<Vec<u8>, Error> {
    let mut encoded = Vec::with_capacity(alephium_crypto::PUBLIC_KEY_LENGTH + 8);
    encoded.extend_from_slice(&info.clique_id.bytes);
    encoded.extend_from_slice(&compact::encode_i32(info.broker_id));
    encoded.extend_from_slice(&compact::encode_i32(info.broker_num));
    Ok(encoded)
}

fn decode_inter_broker_info(input: &[u8]) -> DecodeResult<'_, InterBrokerInfo> {
    let clique_id = fixed::decode_array::<{ alephium_crypto::PUBLIC_KEY_LENGTH }>(input)?;
    let broker_id = compact::decode_i32(clique_id.rest)?;
    let broker_num = compact::decode_i32(broker_id.rest)?;

    let mut clique_bytes = [0u8; alephium_crypto::PUBLIC_KEY_LENGTH];
    clique_bytes.copy_from_slice(&clique_id.value);
    Ok(Staging::new(
        InterBrokerInfo {
            clique_id: CliqueId {
                bytes: clique_bytes,
            },
            broker_id: broker_id.value,
            broker_num: broker_num.value,
        },
        broker_num.rest,
    ))
}

impl InterBrokerInfo {
    pub fn hash(&self) -> [u8; alephium_crypto::HASH_LENGTH] {
        let encoded = encode_inter_broker_info_body(self).expect("inter broker info serializes");
        alephium_crypto::hash_blake2b_256(&encoded)
    }

    fn validate(&self, configured_groups: u32) -> Result<(), Error> {
        if self.broker_id < 0 || self.broker_id >= self.broker_num {
            return Err(Error::validation("Invalid BrokerInfo"));
        }

        if self.broker_num <= 0 {
            return Err(Error::validation("Invalid BrokerInfo"));
        }

        let configured_groups_i32 = i32::try_from(configured_groups).map_err(|_| {
            Error::validation("configured_groups does not fit in i32 for BrokerInfo validation")
        })?;
        if self.broker_num > configured_groups_i32 || configured_groups_i32 % self.broker_num != 0 {
            return Err(Error::validation("Invalid BrokerInfo"));
        }

        PublicKey::try_from(self.clique_id.bytes.as_ref())
            .map_err(|_| Error::validation("Invalid BrokerInfo: invalid clique public key"))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alephium_serde::compact;

    use super::{RequestId, TxsResponse, encode_txs_response_payload};
    use crate::message::decode_single_txs_response_payload;

    #[test]
    fn empty_txs_response_is_request_id_and_zero_count() {
        let encoded = encode_txs_response_payload(&TxsResponse {
            id: RequestId { value: 9 },
            txs: Vec::new(),
        })
        .expect("encode empty txs response");
        let mut expected = compact::encode_u32(9);
        expected.extend_from_slice(&compact::encode_i32(0));
        assert_eq!(encoded, expected);
    }

    #[test]
    fn single_txs_response_round_trips_through_fast_path_decoder() {
        let response = TxsResponse {
            id: RequestId { value: 4 },
            txs: vec![vec![0x41, 0x42]],
        };
        let encoded = encode_txs_response_payload(&response).expect("encode txs response");
        let decoded = decode_single_txs_response_payload(&encoded)
            .expect("decode txs response")
            .finish(encoded.len())
            .expect("no trailing bytes");
        assert_eq!(decoded, response);
    }
}
