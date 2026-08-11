use core::str;

use alephium_crypto::{self, PublicKey, Signature};
use alephium_serde::{self, compact, fixed, bounded, DecodeResult, Error, Staging};

pub const HANDSHAKE_MAX_CLIENT_ID_BYTES: usize = 256;

pub const HELLO_CODE: i32 = 0;
pub const PING_CODE: i32 = 1;
pub const PONG_CODE: i32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadCode {
    Hello,
    Ping,
    Pong,
}

impl PayloadCode {
    #[must_use]
    pub const fn to_i32(self) -> i32 {
        match self {
            Self::Hello => HELLO_CODE,
            Self::Ping => PING_CODE,
            Self::Pong => PONG_CODE,
        }
    }

    pub const fn from_i32(code: i32) -> Option<Self> {
        match code {
            HELLO_CODE => Some(Self::Hello),
            PING_CODE => Some(Self::Ping),
            PONG_CODE => Some(Self::Pong),
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

impl RequestId {
    pub const fn new(value: u32) -> Self {
        Self { value }
    }
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
    let mut encoded = bounded::encode_bytes(hello.client_id.as_bytes(), HANDSHAKE_MAX_CLIENT_ID_BYTES)?;
    encoded.extend_from_slice(&fixed::encode_i64_be(hello.timestamp));
    encoded.extend_from_slice(&encode_inter_broker_info_body(&hello.broker_info)?);
    encoded.extend_from_slice(&hello.signature);
    Ok(encoded)
}

pub fn decode_hello_payload(
    input: &[u8],
    configured_groups: u32,
) -> Result<Hello, Error> {
    let hello = decode_hello_payload_with_rest(input, configured_groups)?;
    if !hello.rest.is_empty() {
        Err(Error::redundant_bytes(input.len() - hello.rest.len(), input.len()))
    } else {
        Ok(hello.value)
    }
}

fn decode_hello_payload_with_rest(
    input: &[u8],
    configured_groups: u32,
) -> DecodeResult<'_, Hello> {
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

        if !alephium_crypto::verify(
            &self.broker_info.hash(),
            &signature,
            &clique_id,
        ) {
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
            clique_id: CliqueId { bytes: clique_bytes },
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

        PublicKey::try_from(self.clique_id.bytes.as_ref()).map_err(|_| {
            Error::validation("Invalid BrokerInfo: invalid clique public key")
        })?;

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
