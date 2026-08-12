use core::str;

use alephium_crypto::{self, PublicKey, Signature};
use alephium_serde::{self, DecodeResult, Error, Staging, bounded, compact, fixed};

pub const HANDSHAKE_MAX_CLIENT_ID_BYTES: usize = 256;
const HASH_BYTES: usize = alephium_crypto::HASH_LENGTH;
const MAX_REQUEST_VECTOR_ITEMS: usize = 1_048_576;
const MAX_WEIGHT_BYTES: usize = 8_192;
const MAX_TX_VECTOR_ITEMS: usize = 1_048_576;
const HEADER_NONCE_BYTES: usize = 24;
const HEADER_VERSION_BYTES: usize = 1;
const HEADER_TARGET_BYTES: usize = 4;

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
pub enum PayloadCode {
    Hello,
    Ping,
    Pong,
    BlocksRequest,
    BlocksResponse,
    HeadersRequest,
    HeadersResponse,
    InvRequest,
    InvResponse,
    NewBlock,
    NewHeader,
    NewInv,
    NewBlockHash,
    NewTxHashes,
    TxsRequest,
    TxsResponse,
    ChainState,
    HeadersByHeightsRequest,
    HeadersByHeightsResponse,
    BlocksAndUnclesByHeightsRequest,
    BlocksAndUnclesByHeightsResponse,
}

impl PayloadCode {
    #[must_use]
    pub const fn to_i32(self) -> i32 {
        match self {
            Self::Hello => HELLO_CODE,
            Self::Ping => PING_CODE,
            Self::Pong => PONG_CODE,
            Self::BlocksRequest => BLOCKS_REQUEST_CODE,
            Self::BlocksResponse => BLOCKS_RESPONSE_CODE,
            Self::HeadersRequest => HEADERS_REQUEST_CODE,
            Self::HeadersResponse => HEADERS_RESPONSE_CODE,
            Self::InvRequest => INV_REQUEST_CODE,
            Self::InvResponse => INV_RESPONSE_CODE,
            Self::NewBlock => NEW_BLOCK_CODE,
            Self::NewHeader => NEW_HEADER_CODE,
            Self::NewInv => NEW_INV_CODE,
            Self::NewBlockHash => NEW_BLOCK_HASH_CODE,
            Self::NewTxHashes => NEW_TX_HASHES_CODE,
            Self::TxsRequest => TXS_REQUEST_CODE,
            Self::TxsResponse => TXS_RESPONSE_CODE,
            Self::ChainState => CHAIN_STATE_CODE,
            Self::HeadersByHeightsRequest => HEADERS_BY_HEIGHTS_REQUEST_CODE,
            Self::HeadersByHeightsResponse => HEADERS_BY_HEIGHTS_RESPONSE_CODE,
            Self::BlocksAndUnclesByHeightsRequest => BLOCKS_AND_UNCLES_BY_HEIGHTS_REQUEST_CODE,
            Self::BlocksAndUnclesByHeightsResponse => BLOCKS_AND_UNCLES_BY_HEIGHTS_RESPONSE_CODE,
        }
    }

    pub const fn from_i32(code: i32) -> Option<Self> {
        match code {
            HELLO_CODE => Some(Self::Hello),
            PING_CODE => Some(Self::Ping),
            PONG_CODE => Some(Self::Pong),
            BLOCKS_REQUEST_CODE => Some(Self::BlocksRequest),
            BLOCKS_RESPONSE_CODE => Some(Self::BlocksResponse),
            HEADERS_REQUEST_CODE => Some(Self::HeadersRequest),
            HEADERS_RESPONSE_CODE => Some(Self::HeadersResponse),
            INV_REQUEST_CODE => Some(Self::InvRequest),
            INV_RESPONSE_CODE => Some(Self::InvResponse),
            NEW_BLOCK_CODE => Some(Self::NewBlock),
            NEW_HEADER_CODE => Some(Self::NewHeader),
            NEW_INV_CODE => Some(Self::NewInv),
            NEW_BLOCK_HASH_CODE => Some(Self::NewBlockHash),
            NEW_TX_HASHES_CODE => Some(Self::NewTxHashes),
            TXS_REQUEST_CODE => Some(Self::TxsRequest),
            TXS_RESPONSE_CODE => Some(Self::TxsResponse),
            CHAIN_STATE_CODE => Some(Self::ChainState),
            HEADERS_BY_HEIGHTS_REQUEST_CODE => Some(Self::HeadersByHeightsRequest),
            HEADERS_BY_HEIGHTS_RESPONSE_CODE => Some(Self::HeadersByHeightsResponse),
            BLOCKS_AND_UNCLES_BY_HEIGHTS_REQUEST_CODE => {
                Some(Self::BlocksAndUnclesByHeightsRequest)
            }
            BLOCKS_AND_UNCLES_BY_HEIGHTS_RESPONSE_CODE => {
                Some(Self::BlocksAndUnclesByHeightsResponse)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub version: i32,
}

impl Header {
    #[must_use]
    pub const fn encode(self) -> [u8; 4] {
        fixed::encode_i32_be(self.version)
    }

    pub fn decode(input: &[u8]) -> DecodeResult<'_, Self> {
        let staged = fixed::decode_i32_be(input)?;
        if staged.value == crate::message::CURRENT_WIRE_VERSION {
            Ok(staged.map(|value| Self { version: value }))
        } else {
            Err(Error::wrong_format(format!(
                "invalid header version: got {}, expected {}",
                staged.value,
                crate::message::CURRENT_WIRE_VERSION
            )))
        }
    }
}

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

impl CliqueId {
    #[must_use]
    pub const fn as_bytes(self) -> [u8; alephium_crypto::PUBLIC_KEY_LENGTH] {
        self.bytes
    }
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
pub struct ReleaseVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockHeightRange {
    pub from: i32,
    pub to: i32,
    pub step: i32,
}

impl BlockHeightRange {
    pub fn validate(&self) -> Result<(), Error> {
        if self.from >= 0 && self.to >= self.from && self.step >= 1 {
            Ok(())
        } else {
            Err(Error::validation(format!(
                "invalid BlockHeightRange: from={}, to={}, step={}",
                self.from, self.to, self.step
            )))
        }
    }
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
pub struct BlocksRequest {
    pub id: RequestId,
    pub locators: Vec<BlockHash>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlocksResponse {
    pub id: RequestId,
    pub blocks: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadersRequest {
    pub id: RequestId,
    pub locators: Vec<BlockHash>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadersResponse {
    pub id: RequestId,
    pub headers: Vec<Vec<u8>>,
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
pub struct NewBlock {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHeader {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInv {
    pub hashes: Vec<Vec<BlockHash>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBlockHash {
    pub hash: BlockHash,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadersByHeightsRequest {
    pub id: RequestId,
    pub data: Vec<(ChainIndex, BlockHeightRange)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadersByHeightsResponse {
    pub id: RequestId,
    pub headers: Vec<Vec<Vec<u8>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlocksAndUnclesByHeightsRequest {
    pub id: RequestId,
    pub data: Vec<(ChainIndex, BlockHeightRange)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlocksAndUnclesByHeightsResponse {
    pub id: RequestId,
    pub blocks: Vec<Vec<Vec<u8>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum P2PVersion {
    V1,
    V2,
}

pub fn encode_wire_header() -> [u8; 4] {
    compact::encode_i32(crate::message::CURRENT_WIRE_VERSION)
        .try_into()
        .expect("wire version fits in fixed-width i32 encoding")
}

pub fn decode_wire_header(input: &[u8]) -> DecodeResult<'_, i32> {
    let staged = compact::decode_i32(input)?;
    if staged.value == crate::message::CURRENT_WIRE_VERSION {
        Ok(staged)
    } else {
        Err(Error::wrong_format(format!(
            "invalid wire version: {}, expected {}",
            staged.value,
            crate::message::CURRENT_WIRE_VERSION
        )))
    }
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

pub fn encode_blocks_request_payload(request: &BlocksRequest) -> Result<Vec<u8>, Error> {
    let mut encoded = encode_request_id(request.id);
    encoded.extend_from_slice(&encode_block_hash_vector(&request.locators)?);
    Ok(encoded)
}

pub fn decode_blocks_request_payload(input: &[u8]) -> DecodeResult<'_, BlocksRequest> {
    let id = decode_request_id(input)?;
    let locators = decode_block_hash_vector(id.rest)?;
    Ok(locators.map(|locators| BlocksRequest {
        id: RequestId { value: id.value },
        locators,
    }))
}

pub fn encode_blocks_response_payload(response: &BlocksResponse) -> Result<Vec<u8>, Error> {
    let mut encoded = encode_request_id(response.id);
    encoded.extend_from_slice(&encode_raw_byte_vector(&response.blocks)?);
    Ok(encoded)
}

pub fn decode_blocks_response_payload(input: &[u8]) -> DecodeResult<'_, BlocksResponse> {
    let id = decode_request_id(input)?;
    let blocks = decode_length_prefixed_bytes_vector_without_trailing_bytes(id.rest)
        .or_else(|_| decode_raw_byte_vector(id.rest))?;
    Ok(blocks.map(|blocks| BlocksResponse {
        id: RequestId { value: id.value },
        blocks,
    }))
}

pub fn encode_headers_request_payload(request: &HeadersRequest) -> Result<Vec<u8>, Error> {
    let mut encoded = encode_request_id(request.id);
    encoded.extend_from_slice(&encode_block_hash_vector(&request.locators)?);
    Ok(encoded)
}

pub fn decode_headers_request_payload(input: &[u8]) -> DecodeResult<'_, HeadersRequest> {
    let id = decode_request_id(input)?;
    let locators = decode_block_hash_vector(id.rest)?;
    Ok(locators.map(|locators| HeadersRequest {
        id: RequestId { value: id.value },
        locators,
    }))
}

pub fn encode_headers_response_payload(response: &HeadersResponse) -> Result<Vec<u8>, Error> {
    let mut encoded = encode_request_id(response.id);
    encoded.extend_from_slice(&encode_headers_vector(&response.headers)?);
    Ok(encoded)
}

pub fn decode_headers_response_payload(input: &[u8]) -> DecodeResult<'_, HeadersResponse> {
    let id = decode_request_id(input)?;
    let headers = decode_headers_vector(id.rest)
        .or_else(|_| decode_length_prefixed_bytes_vector(id.rest))?;
    Ok(headers.map(|headers| HeadersResponse {
        id: RequestId { value: id.value },
        headers,
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

pub fn encode_new_block_payload(block: &NewBlock) -> Vec<u8> {
    block.bytes.clone()
}

pub fn decode_new_block_payload(input: &[u8]) -> DecodeResult<'_, NewBlock> {
    Ok(Staging::new(
        NewBlock {
            bytes: input.to_vec(),
        },
        &[],
    ))
}

pub fn encode_new_header_payload(header: &NewHeader) -> Vec<u8> {
    header.bytes.clone()
}

pub fn decode_new_header_payload(input: &[u8]) -> DecodeResult<'_, NewHeader> {
    Ok(Staging::new(
        NewHeader {
            bytes: input.to_vec(),
        },
        &[],
    ))
}

pub fn encode_new_inv_payload(new_inv: &NewInv) -> Result<Vec<u8>, Error> {
    encode_nested_block_hash_vector(&new_inv.hashes)
}

pub fn decode_new_inv_payload(input: &[u8]) -> DecodeResult<'_, NewInv> {
    let hashes = decode_nested_block_hash_vector(input)?;
    Ok(hashes.map(|hashes| NewInv { hashes }))
}

pub fn encode_new_block_hash_payload(block_hash: &NewBlockHash) -> Vec<u8> {
    block_hash.hash.bytes.to_vec()
}

pub fn decode_new_block_hash_payload(input: &[u8]) -> DecodeResult<'_, NewBlockHash> {
    let staged = fixed::decode_array::<HASH_BYTES>(input)?;
    Ok(staged.map(|hash| NewBlockHash {
        hash: BlockHash { bytes: hash },
    }))
}

pub fn encode_new_txhashes_payload(new_tx_hashes: &NewTxHashes) -> Result<Vec<u8>, Error> {
    encode_chain_indexed_txid_vector(&new_tx_hashes.hashes)
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

pub fn decode_txs_response_payload(input: &[u8]) -> DecodeResult<'_, TxsResponse> {
    let id = decode_request_id(input)?;
    let txs = decode_length_prefixed_bytes_vector_without_trailing_bytes(id.rest)
        .or_else(|_| decode_raw_byte_vector(id.rest))?;
    Ok(txs.map(|txs| TxsResponse {
        id: RequestId { value: id.value },
        txs,
    }))
}

pub fn encode_chain_state_payload(state: &ChainState) -> Result<Vec<u8>, Error> {
    let encoded = encode_chain_tips(&state.tips)?;
    Ok(encoded)
}

pub fn decode_chain_state_payload(input: &[u8]) -> DecodeResult<'_, ChainState> {
    let tips = decode_chain_tips(input)?;
    Ok(tips.map(ChainState::from))
}

pub fn encode_headers_by_heights_request_payload(
    request: &HeadersByHeightsRequest,
) -> Result<Vec<u8>, Error> {
    let mut encoded = encode_request_id(request.id);
    encoded.extend_from_slice(&encode_chain_index_range_vector(&request.data)?);
    Ok(encoded)
}

pub fn decode_headers_by_heights_request_payload(
    input: &[u8],
) -> DecodeResult<'_, HeadersByHeightsRequest> {
    let id = decode_request_id(input)?;
    let data = decode_chain_index_range_vector(id.rest)?;
    Ok(data.map(|data| HeadersByHeightsRequest {
        id: RequestId { value: id.value },
        data,
    }))
}

pub fn encode_headers_by_heights_response_payload(
    response: &HeadersByHeightsResponse,
) -> Result<Vec<u8>, Error> {
    let mut encoded = encode_request_id(response.id);
    encoded.extend_from_slice(&encode_nested_headers_vector(&response.headers)?);
    Ok(encoded)
}

pub fn decode_headers_by_heights_response_payload(
    input: &[u8],
) -> DecodeResult<'_, HeadersByHeightsResponse> {
    let id = decode_request_id(input)?;
    let headers = decode_nested_headers_vector(id.rest).or_else(|_| decode_byte_matrix(id.rest))?;
    Ok(headers.map(|headers| HeadersByHeightsResponse {
        id: RequestId { value: id.value },
        headers,
    }))
}

pub fn encode_blocks_and_uncles_by_heights_request_payload(
    request: &BlocksAndUnclesByHeightsRequest,
) -> Result<Vec<u8>, Error> {
    let mut encoded = encode_request_id(request.id);
    encoded.extend_from_slice(&encode_chain_index_range_vector(&request.data)?);
    Ok(encoded)
}

pub fn decode_blocks_and_uncles_by_heights_request_payload(
    input: &[u8],
) -> DecodeResult<'_, BlocksAndUnclesByHeightsRequest> {
    let id = decode_request_id(input)?;
    let data = decode_chain_index_range_vector(id.rest)?;
    Ok(data.map(|data| BlocksAndUnclesByHeightsRequest {
        id: RequestId { value: id.value },
        data,
    }))
}

pub fn encode_blocks_and_uncles_by_heights_response_payload(
    response: &BlocksAndUnclesByHeightsResponse,
) -> Result<Vec<u8>, Error> {
    let mut encoded = encode_request_id(response.id);
    encoded.extend_from_slice(&encode_byte_matrix(&response.blocks)?);
    Ok(encoded)
}

pub fn decode_blocks_and_uncles_by_heights_response_payload(
    input: &[u8],
) -> DecodeResult<'_, BlocksAndUnclesByHeightsResponse> {
    let id = decode_request_id(input)?;
    let blocks = decode_byte_matrix(id.rest)?;
    Ok(blocks.map(|blocks| BlocksAndUnclesByHeightsResponse {
        id: RequestId { value: id.value },
        blocks,
    }))
}

impl ChainState {
    #[must_use]
    fn from(tips: Vec<ChainTip>) -> Self {
        Self { tips }
    }
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

fn encode_block_height_range(range: &BlockHeightRange) -> Result<Vec<u8>, Error> {
    range.validate()?;
    let mut encoded = compact::encode_i32(range.from);
    encoded.extend_from_slice(&compact::encode_i32(range.to));
    encoded.extend_from_slice(&compact::encode_i32(range.step));
    Ok(encoded)
}

fn decode_block_height_range(input: &[u8]) -> DecodeResult<'_, BlockHeightRange> {
    let from = compact::decode_i32(input)?;
    let to = compact::decode_i32(from.rest)?;
    let step = compact::decode_i32(to.rest)?;
    let range = BlockHeightRange {
        from: from.value,
        to: to.value,
        step: step.value,
    };
    range.validate()?;
    Ok(Staging::new(range, step.rest))
}

fn encode_weight(weight: &[u8]) -> Result<Vec<u8>, Error> {
    if weight.len() > MAX_WEIGHT_BYTES {
        return Err(Error::LimitExceeded {
            declared: weight.len(),
            limit: MAX_WEIGHT_BYTES,
        });
    }
    let mut encoded =
        compact::encode_i32(
            i32::try_from(weight.len()).map_err(|_| Error::LimitExceeded {
                declared: weight.len(),
                limit: i32::MAX as usize,
            })?,
        );
    encoded.extend_from_slice(weight);
    Ok(encoded)
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

fn encode_chain_tip(tip: &ChainTip) -> Result<Vec<u8>, Error> {
    tip.validate()?;
    let mut encoded = encode_hash(&tip.hash);
    encoded.extend_from_slice(&fixed::encode_i32_be(tip.height));
    encoded.extend_from_slice(&encode_weight(&tip.weight)?);
    Ok(encoded)
}

fn decode_chain_tip(input: &[u8]) -> DecodeResult<'_, ChainTip> {
    let hash = decode_hash(input)?;
    let height = fixed::decode_i32_be(hash.rest)?;
    let weight = decode_weight(height.rest)?;
    let tip = ChainTip {
        hash: hash.value,
        height: height.value,
        weight: weight.value.clone(),
    };
    tip.validate()?;
    Ok(Staging::new(tip, weight.rest))
}

fn encode_chain_tips(tips: &[ChainTip]) -> Result<Vec<u8>, Error> {
    let mut encoded = bounded::encode_vec(tips, MAX_REQUEST_VECTOR_ITEMS, |tip, encoded| {
        tip.validate()?;
        encoded.extend_from_slice(&encode_chain_tip(tip)?);
        Ok(())
    })?;
    if encoded.is_empty() {
        // Keep empty vector encoding as a zero-length compact int.
        encoded = compact::encode_i32(0);
    }
    Ok(encoded)
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

fn encode_byte_vector(values: &[Vec<u8>]) -> Result<Vec<u8>, Error> {
    bounded::encode_vec(values, MAX_REQUEST_VECTOR_ITEMS, |value, encoded| {
        encoded.extend_from_slice(&compact::encode_i32(i32::try_from(value.len()).map_err(
            |_| Error::LimitExceeded {
                declared: value.len(),
                limit: MAX_REQUEST_VECTOR_ITEMS,
            },
        )?));
        if value.len() > MAX_TX_VECTOR_ITEMS {
            return Err(Error::LimitExceeded {
                declared: value.len(),
                limit: MAX_TX_VECTOR_ITEMS,
            });
        }
        encoded.extend_from_slice(value);
        Ok(())
    })
}

fn encode_raw_byte_vector(values: &[Vec<u8>]) -> Result<Vec<u8>, Error> {
    let mut encoded = compact::encode_i32(i32::try_from(values.len()).map_err(|_| {
        Error::LimitExceeded {
            declared: values.len(),
            limit: MAX_REQUEST_VECTOR_ITEMS,
        }
    })?);
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

fn encode_headers_vector(values: &[Vec<u8>]) -> Result<Vec<u8>, Error> {
    let mut encoded = compact::encode_i32(i32::try_from(values.len()).map_err(|_| {
        Error::LimitExceeded {
            declared: values.len(),
            limit: MAX_REQUEST_VECTOR_ITEMS,
        }
    })?);
    for value in values {
        if value.len() > MAX_REQUEST_VECTOR_ITEMS {
            return Err(Error::LimitExceeded {
                declared: value.len(),
                limit: MAX_REQUEST_VECTOR_ITEMS,
            });
        }
        encoded.extend_from_slice(value);
    }
    Ok(encoded)
}

fn decode_length_prefixed_bytes_vector(input: &[u8]) -> DecodeResult<'_, Vec<Vec<u8>>> {
    let staged_len = compact::decode_i32(input)?;
    if staged_len.value < 0 {
        return Err(Error::validation(format!(
            "negative byte vector count: {}",
            staged_len.value
        )));
    }
    let len = usize::try_from(staged_len.value).map_err(|_| {
        Error::validation(format!(
            "invalid byte vector count conversion: {}",
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
    let mut items = Vec::with_capacity(len);
    for _ in 0..len {
        let staged_len = compact::decode_i32(rest)?;
        if staged_len.value < 0 {
            return Err(Error::validation(format!(
                "negative byte payload length: {}",
                staged_len.value
            )));
        }
        let item_len = usize::try_from(staged_len.value).map_err(|_| {
            Error::validation(format!(
                "invalid byte payload length conversion: {}",
                staged_len.value
            ))
        })?;
        if item_len > MAX_TX_VECTOR_ITEMS {
            return Err(Error::LimitExceeded {
                declared: item_len,
                limit: MAX_TX_VECTOR_ITEMS,
            });
        }
        let staged = fixed::take(staged_len.rest, item_len)?;
        let bytes = staged.value.to_vec();
        items.push(bytes);
        rest = staged.rest;
    }
    Ok(Staging::new(items, rest))
}

fn decode_length_prefixed_bytes_vector_without_trailing_bytes(
    input: &[u8],
) -> DecodeResult<'_, Vec<Vec<u8>>> {
    decode_length_prefixed_bytes_vector(input).and_then(|staged| {
        if staged.rest.is_empty() {
            Ok(staged)
        } else {
            Err(Error::validation(
                "length-prefixed payload has trailing bytes".to_owned(),
            ))
        }
    })
}

fn decode_raw_byte_vector(input: &[u8]) -> DecodeResult<'_, Vec<Vec<u8>>> {
    let staged_len = compact::decode_i32(input)?;
    if staged_len.value < 0 {
        return Err(Error::validation(format!(
            "negative byte vector count: {}",
            staged_len.value
        )));
    }
    let len = usize::try_from(staged_len.value).map_err(|_| {
        Error::validation(format!(
            "invalid byte vector count conversion: {}",
            staged_len.value
        ))
    })?;
    if len > MAX_TX_VECTOR_ITEMS {
        return Err(Error::LimitExceeded {
            declared: len,
            limit: MAX_TX_VECTOR_ITEMS,
        });
    }

    if len == 0 {
        return Ok(Staging::new(Vec::new(), staged_len.rest));
    }

    if len == 1 {
        return Ok(Staging::new(vec![staged_len.rest.to_vec()], &[]));
    }

    if staged_len.rest.is_empty() {
        return Err(Error::validation(format!(
            "raw byte vector declared {len} items but no payload bytes were found"
        )));
    }

    Ok(Staging::new(vec![staged_len.rest.to_vec()], &[]))
}

fn decode_headers_vector(input: &[u8]) -> DecodeResult<'_, Vec<Vec<u8>>> {
    let staged_len = compact::decode_i32(input)?;
    if staged_len.value < 0 {
        return Err(Error::validation(format!(
            "negative header vector count: {}",
            staged_len.value
        )));
    }
    let len = usize::try_from(staged_len.value).map_err(|_| {
        Error::validation(format!(
            "invalid header vector count conversion: {}",
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
    let mut headers = Vec::with_capacity(len);
    for _ in 0..len {
        let header_len = decode_block_header_len(rest)?;
        let staged = fixed::take(rest, header_len)?;
        headers.push(staged.value.to_vec());
        rest = staged.rest;
    }
    Ok(Staging::new(headers, rest))
}

fn encode_nested_headers_vector(values: &[Vec<Vec<u8>>]) -> Result<Vec<u8>, Error> {
    bounded::encode_vec(values, MAX_REQUEST_VECTOR_ITEMS, |row, encoded| {
        encoded.extend_from_slice(&compact::encode_i32(i32::try_from(row.len()).map_err(
            |_| Error::LimitExceeded {
                declared: row.len(),
                limit: MAX_REQUEST_VECTOR_ITEMS,
            },
        )?));
        for header in row {
            encoded.extend_from_slice(header);
        }
        Ok(())
    })
}

fn decode_nested_headers_vector(input: &[u8]) -> DecodeResult<'_, Vec<Vec<Vec<u8>>>> {
    let staged_chain_count = compact::decode_i32(input)?;
    if staged_chain_count.value < 0 {
        return Err(Error::validation(format!(
            "negative headers-by-heights chain count: {}",
            staged_chain_count.value
        )));
    }
    let chain_count = usize::try_from(staged_chain_count.value).map_err(|_| {
        Error::validation(format!(
            "invalid headers-by-heights chain count conversion: {}",
            staged_chain_count.value
        ))
    })?;
    if chain_count > MAX_REQUEST_VECTOR_ITEMS {
        return Err(Error::LimitExceeded {
            declared: chain_count,
            limit: MAX_REQUEST_VECTOR_ITEMS,
        });
    }

    let mut rest = staged_chain_count.rest;
    let mut headers = Vec::with_capacity(chain_count);
    for _ in 0..chain_count {
        let chain_headers = decode_headers_vector(rest)?;
        headers.push(chain_headers.value);
        rest = chain_headers.rest;
    }
    Ok(Staging::new(headers, rest))
}

fn decode_block_header_len(input: &[u8]) -> Result<usize, Error> {
    let mut rest = input;

    rest = fixed::take(rest, HEADER_NONCE_BYTES).map_err(|_| {
        Error::validation("invalid block header length: truncated nonce".to_owned())
    })?
    .rest;
    if rest.len() < HEADER_VERSION_BYTES {
        return Err(Error::validation(
            "invalid block header length: truncated version byte".to_owned(),
        ));
    }
    rest = &rest[HEADER_VERSION_BYTES..];

    let deps_count = compact::decode_i32(rest)
        .map_err(|_| Error::validation("invalid block header length: invalid dep count".to_owned()))?;
    if deps_count.value < 0 {
        return Err(Error::validation(format!(
            "negative block header deps count: {}",
            deps_count.value
        )));
    }
    rest = deps_count.rest;
    let deps_count = usize::try_from(deps_count.value).map_err(|_| {
        Error::validation(format!(
            "invalid block header deps count conversion: {}",
            deps_count.value
        ))
    })?;
    if deps_count > MAX_REQUEST_VECTOR_ITEMS {
        return Err(Error::LimitExceeded {
            declared: deps_count,
            limit: MAX_REQUEST_VECTOR_ITEMS,
        });
    }

    for _ in 0..deps_count {
        rest = fixed::take(rest, HASH_BYTES).map_err(|_| {
            Error::validation("invalid block header length: truncated deps hash".to_owned())
        })?
        .rest;
    }
    rest = fixed::take(rest, HASH_BYTES)
        .map_err(|_| {
            Error::validation("invalid block header length: truncated dep-state hash".to_owned())
        })?
        .rest;
    rest = fixed::take(rest, HASH_BYTES)
        .map_err(|_| {
            Error::validation("invalid block header length: truncated txs hash".to_owned())
        })?
        .rest;
    let _timestamp = fixed::decode_i64_be(rest)
        .map_err(|_| Error::validation("invalid block header length: invalid timestamp".to_owned()))?;
    rest = _timestamp.rest;
    rest = fixed::take(rest, HEADER_TARGET_BYTES)
        .map_err(|_| {
            Error::validation("invalid block header length: truncated target".to_owned())
        })?
        .rest;

    input
        .len()
        .checked_sub(rest.len())
        .ok_or_else(|| Error::validation("invalid block header length: exhausted while decoding".to_owned()))
        .and_then(|consumed| {
            if consumed == 0 {
                Err(Error::validation(
                    "invalid block header length: zero-length header".to_owned(),
                ))
            } else {
                Ok(consumed)
            }
        })
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

fn encode_chain_index_range_vector(
    values: &[(ChainIndex, BlockHeightRange)],
) -> Result<Vec<u8>, Error> {
    bounded::encode_vec(
        values,
        MAX_REQUEST_VECTOR_ITEMS,
        |(index, range), encoded| {
            encoded.extend_from_slice(&encode_chain_index(index)?);
            encoded.extend_from_slice(&encode_block_height_range(range)?);
            Ok(())
        },
    )
}

fn decode_chain_index_range_vector(
    input: &[u8],
) -> DecodeResult<'_, Vec<(ChainIndex, BlockHeightRange)>> {
    let staged_len = compact::decode_i32(input)?;
    if staged_len.value < 0 {
        return Err(Error::validation(format!(
            "negative chain-range count: {}",
            staged_len.value
        )));
    }
    let len = usize::try_from(staged_len.value).map_err(|_| {
        Error::validation(format!(
            "invalid chain-range count conversion: {}",
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
        let range = decode_block_height_range(index.rest)?;
        values.push((index.value, range.value));
        rest = range.rest;
    }
    Ok(Staging::new(values, rest))
}

fn encode_byte_matrix(values: &[Vec<Vec<u8>>]) -> Result<Vec<u8>, Error> {
    bounded::encode_vec(values, MAX_REQUEST_VECTOR_ITEMS, |row, encoded| {
        encoded.extend_from_slice(&encode_byte_vector(row)?);
        Ok(())
    })
}

fn decode_byte_matrix(input: &[u8]) -> DecodeResult<'_, Vec<Vec<Vec<u8>>>> {
    let staged_len = compact::decode_i32(input)?;
    if staged_len.value < 0 {
        return Err(Error::validation(format!(
            "negative byte matrix outer count: {}",
            staged_len.value
        )));
    }
    let len = usize::try_from(staged_len.value).map_err(|_| {
        Error::validation(format!(
            "invalid byte matrix outer count conversion: {}",
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
    let mut rows = Vec::with_capacity(len);
    for _ in 0..len {
        let row = decode_length_prefixed_bytes_vector(rest)?;
        rows.push(row.value);
        rest = row.rest;
    }
    Ok(Staging::new(rows, rest))
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

impl ReleaseVersion {
    pub fn parse(version_text: &str) -> Option<Self> {
        let stripped = version_text.strip_prefix('v').unwrap_or(version_text);
        let mut parts = stripped.split('.');
        let major = parts.next()?.parse::<u32>().ok()?;
        let minor = parts.next()?.parse::<u32>().ok()?;
        let patch_text = parts.next()?;
        let patch = patch_text
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .ok()?;
        Some(Self {
            major,
            minor,
            patch,
        })
    }

    pub fn from_client_id(client_id: &str) -> Option<Self> {
        let mut split = client_id.split('/');
        split.next()?;
        split.next().and_then(Self::parse)
    }
}

impl P2PVersion {
    pub fn from_client_id(client_id: &str) -> Option<Self> {
        let parts: Vec<_> = client_id.split('/').collect();
        match parts.len() {
            3 => Some(Self::V1),
            4 => match parts[3] {
                "p2p-v1" => Some(Self::V1),
                "p2p-v2" => Some(Self::V2),
                _ => None,
            },
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BlocksResponse, HeadersResponse, RequestId, TxsResponse, decode_blocks_response_payload,
        decode_headers_by_heights_response_payload, decode_headers_response_payload,
        decode_txs_response_payload,
        encode_blocks_response_payload, encode_headers_response_payload,
        encode_txs_response_payload,
    };
    use alephium_serde::compact;

    #[test]
    fn response_payload_round_trips_are_exact() {
        let headers_response = HeadersResponse {
            id: RequestId { value: 2 },
            headers: vec![fake_block_header()],
        };
        let encoded_headers = encode_headers_response_payload(&headers_response)
            .expect("encode headers response fixture");
        let decoded_headers = decode_headers_response_payload(&encoded_headers)
            .expect("decode headers response fixture");
        assert_eq!(decoded_headers.rest, &[]);
        assert_eq!(decoded_headers.value, headers_response);

        let blocks_response = BlocksResponse {
            id: RequestId { value: 3 },
            blocks: vec![vec![0x31_u8, 0x32_u8]],
        };
        let encoded_blocks = encode_blocks_response_payload(&blocks_response)
            .expect("encode blocks response fixture");
        let decoded_blocks = decode_blocks_response_payload(&encoded_blocks)
            .expect("decode blocks response fixture");
        assert_eq!(decoded_blocks.rest, &[]);
        assert_eq!(decoded_blocks.value, blocks_response);

        let txs_response = TxsResponse {
            id: RequestId { value: 4 },
            txs: vec![vec![0x41_u8, 0x42_u8]],
        };
        let encoded_txs =
            encode_txs_response_payload(&txs_response).expect("encode txs response fixture");
        let decoded_txs =
            decode_txs_response_payload(&encoded_txs).expect("decode txs response fixture");
        assert_eq!(decoded_txs.rest, &[]);
        assert_eq!(decoded_txs.value, txs_response);
    }

    #[test]
    fn decode_blocks_response_legacy_compatibility() {
        let mut blocks_payload = compact::encode_u32(2);
        blocks_payload.extend_from_slice(&compact::encode_i32(1));
        blocks_payload.extend_from_slice(&compact::encode_i32(2));
        blocks_payload.extend_from_slice(&[0x21_u8, 0x22_u8]);
        let decoded_blocks = decode_blocks_response_payload(&blocks_payload)
            .expect("decode legacy blocks response");
        assert_eq!(decoded_blocks.value.id.value, 2);
        assert_eq!(decoded_blocks.value.blocks, vec![vec![0x21_u8, 0x22_u8]]);
    }

    #[test]
    fn decode_blocks_response_raw_singleton() {
        let mut blocks_payload = compact::encode_u32(3);
        blocks_payload.push(1);
        blocks_payload.extend_from_slice(&[0x33_u8, 0x34_u8, 0x35_u8]);
        let decoded_blocks = decode_blocks_response_payload(&blocks_payload)
            .expect("decode official-style raw singleton blocks response");
        assert_eq!(decoded_blocks.value.id.value, 3);
        assert_eq!(
            decoded_blocks.value.blocks,
            vec![vec![0x33_u8, 0x34_u8, 0x35_u8]]
        );
    }

    #[test]
    fn decode_blocks_response_raw_multiitem() {
        let mut blocks_payload = compact::encode_u32(3);
        blocks_payload.push(2);
        blocks_payload.extend_from_slice(&[0x80_u8, 0x00_u8, 0x11_u8, 0x22_u8, 0x33_u8, 0x44_u8]);
        let decoded_blocks = decode_blocks_response_payload(&blocks_payload)
            .expect("decode official-style raw multi-item blocks response");
        assert_eq!(decoded_blocks.value.id.value, 3);
        assert_eq!(decoded_blocks.value.blocks.len(), 1);
        assert_eq!(
            decoded_blocks.value.blocks,
            vec![vec![0x80_u8, 0x00_u8, 0x11_u8, 0x22_u8, 0x33_u8, 0x44_u8]]
        );
    }

    #[test]
    fn decode_txs_response_legacy_compatibility() {
        let mut txs_payload = compact::encode_u32(4);
        txs_payload.extend_from_slice(&compact::encode_i32(1));
        txs_payload.extend_from_slice(&compact::encode_i32(2));
        txs_payload.extend_from_slice(&[0x11_u8, 0x12_u8]);
        let decoded_txs = decode_txs_response_payload(&txs_payload).expect("decode legacy txs response");
        assert_eq!(decoded_txs.value.id.value, 4);
        assert_eq!(decoded_txs.value.txs, vec![vec![0x11_u8, 0x12_u8]]);
    }

    #[test]
    fn decode_txs_response_raw_singleton() {
        let mut txs_payload = compact::encode_u32(5);
        txs_payload.push(1);
        txs_payload.extend_from_slice(&[0x44_u8, 0x45_u8, 0x46_u8]);
        let decoded_txs = decode_txs_response_payload(&txs_payload)
            .expect("decode official-style raw singleton txs response");
        assert_eq!(decoded_txs.value.id.value, 5);
        assert_eq!(decoded_txs.value.txs, vec![vec![0x44_u8, 0x45_u8, 0x46_u8]]);
    }

    #[test]
    fn decode_headers_response_legacy_compatibility() {
        let mut headers_payload = compact::encode_u32(2);
        headers_payload.extend_from_slice(&compact::encode_i32(1));
        headers_payload.extend_from_slice(&compact::encode_i32(2));
        headers_payload.extend_from_slice(&[0x21_u8, 0x22_u8]);
        let decoded_headers = decode_headers_response_payload(&headers_payload)
            .expect("decode legacy headers response");
        assert_eq!(decoded_headers.value.id.value, 2);
        assert_eq!(decoded_headers.value.headers, vec![vec![0x21_u8, 0x22_u8]]);
    }

    #[test]
    fn decode_headers_response_official_peer_payload() {
        let payload = vec![
            2, 1, 60, 119, 215, 77, 17, 53, 106, 228, 29, 87, 237, 189, 228, 240, 35, 244,
            119, 78, 78, 183, 232, 254, 243, 160, 0, 7, 224, 102, 224, 223, 92, 50, 12, 236,
            113, 213, 87, 236, 1, 197, 89, 95, 146, 1, 122, 73, 110, 177, 132, 162, 131, 175,
            25, 81, 222, 9, 217, 69, 157, 205, 89, 97, 165, 219, 144, 146, 38, 238, 140, 20,
            206, 6, 202, 219, 38, 107, 167, 113, 204, 130, 90, 188, 81, 73, 118, 219, 131, 171,
            123, 138, 49, 193, 123, 204, 25, 48, 78, 47, 37, 115, 203, 234, 150, 61, 240, 243,
            146, 144, 27, 212, 179, 189, 33, 91, 216, 207, 149, 27, 137, 150, 84, 223, 99,
            207, 161, 251, 70, 23, 184, 122, 104, 158, 179, 28, 50, 39, 98, 116, 28, 234, 35,
            245, 105, 183, 69, 179, 91, 125, 158, 72, 149, 79, 222, 240, 139, 199, 217, 77,
            223, 198, 97, 41, 4, 152, 98, 229, 1, 255, 93, 80, 66, 225, 233, 120, 177, 165,
            29, 40, 226, 56, 50, 20, 68, 169, 16, 113, 88, 150, 238, 60, 188, 140, 79, 68, 50,
            160, 178, 33, 3, 145, 19, 6, 27, 108, 247, 238, 183, 148, 178, 87, 88, 210, 71,
            18, 13, 13, 247, 18, 252, 49, 43, 12, 4, 160, 238, 176, 118, 126, 201, 129, 55,
            183, 64, 4, 125, 124, 94, 143, 100, 70, 53, 49, 130, 139, 112, 21, 203, 234, 234,
            163, 208, 3, 60, 245, 44, 103, 59, 39, 44, 53, 94, 188, 243, 184, 72, 80, 132,
            117, 123, 89, 226, 65, 181, 72, 214, 121, 193, 81, 74, 163, 182, 50, 211, 173,
            121, 71, 173, 130, 230, 203, 24, 1, 87, 62, 19, 63, 125, 86, 22, 19, 124, 120, 64,
            229, 144, 173, 219, 108, 185, 254, 140, 10, 150, 112, 0, 0, 1, 159, 244, 17, 214,
            239, 32, 255, 255, 255,
        ];

        let decoded = decode_headers_response_payload(&payload)
            .expect("decode official peer headers response payload");
        assert_eq!(decoded.value.id.value, 2);
        assert_eq!(decoded.value.headers.len(), 1);
        assert_eq!(decoded.value.headers[0].len(), 326);
        assert_eq!(decoded.rest, &[]);
    }

    #[test]
    fn decode_headers_by_heights_response_legacy_compatibility() {
        let mut headers_payload = compact::encode_u32(2);
        headers_payload.extend_from_slice(&compact::encode_i32(1));
        headers_payload.extend_from_slice(&compact::encode_i32(1));
        headers_payload.extend_from_slice(&compact::encode_i32(2));
        headers_payload.extend_from_slice(&[0x21_u8, 0x22_u8]);
        let decoded_headers =
            decode_headers_by_heights_response_payload(&headers_payload)
                .expect("decode legacy headers-by-heights response");
        assert_eq!(decoded_headers.value.id.value, 2);
        assert_eq!(decoded_headers.value.headers, vec![vec![vec![0x21_u8, 0x22_u8]]]);
    }

    fn fake_block_header() -> Vec<u8> {
        let mut header = Vec::with_capacity(24 + 1 + 4 + 2 * 32 + 8 + 4);
        header.extend_from_slice(&[0_u8; 24]);
        header.push(0x00);
        header.extend_from_slice(&compact::encode_i32(0));
        header.extend_from_slice(&[0_u8; 32]);
        header.extend_from_slice(&[0_u8; 32]);
        header.extend_from_slice(&0_i64.to_be_bytes());
        header.extend_from_slice(&[0_u8; 4]);
        header
    }
}
