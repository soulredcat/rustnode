use std::fmt;

pub const EVENT_MAGIC: [u8; 4] = *b"ALFP";
pub const EVENT_PROTOCOL_VERSION: u16 = 1;
pub const EVENT_HEADER_LEN: usize = 68;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EventKind {
    PeerConnected = 1,
    TransactionAnnounced = 2,
    RawTransaction = 3,
    PeerDisconnected = 4,
    Heartbeat = 5,
}

impl EventKind {
    fn from_u8(value: u8) -> Result<Self, EventDecodeError> {
        match value {
            1 => Ok(Self::PeerConnected),
            2 => Ok(Self::TransactionAnnounced),
            3 => Ok(Self::RawTransaction),
            4 => Ok(Self::PeerDisconnected),
            5 => Ok(Self::Heartbeat),
            _ => Err(EventDecodeError::UnknownKind(value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotEvent {
    pub kind: EventKind,
    pub flags: u8,
    pub sequence: u64,
    pub observed_at_unix_ns: u64,
    pub chain_from: i32,
    pub chain_to: i32,
    pub transaction_id: [u8; 32],
    pub payload: Vec<u8>,
}

impl BotEvent {
    #[must_use]
    pub fn peer_connected(peer_address: &str, observed_at_unix_ns: u64) -> Self {
        Self {
            kind: EventKind::PeerConnected,
            flags: 0,
            sequence: 0,
            observed_at_unix_ns,
            chain_from: -1,
            chain_to: -1,
            transaction_id: [0; 32],
            payload: peer_address.as_bytes().to_vec(),
        }
    }

    #[must_use]
    pub const fn transaction_announced(
        chain_from: i32,
        chain_to: i32,
        transaction_id: [u8; 32],
        observed_at_unix_ns: u64,
    ) -> Self {
        Self {
            kind: EventKind::TransactionAnnounced,
            flags: 0,
            sequence: 0,
            observed_at_unix_ns,
            chain_from,
            chain_to,
            transaction_id,
            payload: Vec::new(),
        }
    }

    #[must_use]
    pub fn raw_transaction(
        chain_from: i32,
        chain_to: i32,
        transaction_id: [u8; 32],
        raw_transaction: Vec<u8>,
        observed_at_unix_ns: u64,
    ) -> Self {
        Self {
            kind: EventKind::RawTransaction,
            flags: 0,
            sequence: 0,
            observed_at_unix_ns,
            chain_from,
            chain_to,
            transaction_id,
            payload: raw_transaction,
        }
    }

    #[must_use]
    pub fn peer_disconnected(reason: &str, observed_at_unix_ns: u64) -> Self {
        Self {
            kind: EventKind::PeerDisconnected,
            flags: 0,
            sequence: 0,
            observed_at_unix_ns,
            chain_from: -1,
            chain_to: -1,
            transaction_id: [0; 32],
            payload: reason.as_bytes().to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventEncodeError {
    PayloadTooLarge(usize),
}

impl fmt::Display for EventEncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadTooLarge(length) => {
                write!(formatter, "event payload exceeds u32 length: {length}")
            }
        }
    }
}

impl std::error::Error for EventEncodeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventDecodeError {
    Truncated { expected: usize, actual: usize },
    InvalidMagic([u8; 4]),
    UnsupportedVersion(u16),
    UnknownKind(u8),
    TrailingBytes { expected: usize, actual: usize },
}

impl fmt::Display for EventDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { expected, actual } => {
                write!(formatter, "truncated event: expected {expected}, got {actual}")
            }
            Self::InvalidMagic(magic) => write!(formatter, "invalid event magic: {magic:?}"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported event version: {version}")
            }
            Self::UnknownKind(kind) => write!(formatter, "unknown event kind: {kind}"),
            Self::TrailingBytes { expected, actual } => {
                write!(formatter, "event length mismatch: expected {expected}, got {actual}")
            }
        }
    }
}

impl std::error::Error for EventDecodeError {}

pub fn encode_event(event: &BotEvent) -> Result<Vec<u8>, EventEncodeError> {
    let payload_len = u32::try_from(event.payload.len())
        .map_err(|_| EventEncodeError::PayloadTooLarge(event.payload.len()))?;
    let mut output = Vec::with_capacity(EVENT_HEADER_LEN + event.payload.len());
    output.extend_from_slice(&EVENT_MAGIC);
    output.extend_from_slice(&EVENT_PROTOCOL_VERSION.to_be_bytes());
    output.push(event.kind as u8);
    output.push(event.flags);
    output.extend_from_slice(&event.sequence.to_be_bytes());
    output.extend_from_slice(&event.observed_at_unix_ns.to_be_bytes());
    output.extend_from_slice(&event.chain_from.to_be_bytes());
    output.extend_from_slice(&event.chain_to.to_be_bytes());
    output.extend_from_slice(&event.transaction_id);
    output.extend_from_slice(&payload_len.to_be_bytes());
    output.extend_from_slice(&event.payload);
    Ok(output)
}

pub fn decode_event(input: &[u8]) -> Result<BotEvent, EventDecodeError> {
    if input.len() < EVENT_HEADER_LEN {
        return Err(EventDecodeError::Truncated {
            expected: EVENT_HEADER_LEN,
            actual: input.len(),
        });
    }

    let magic = [input[0], input[1], input[2], input[3]];
    if magic != EVENT_MAGIC {
        return Err(EventDecodeError::InvalidMagic(magic));
    }

    let version = u16::from_be_bytes([input[4], input[5]]);
    if version != EVENT_PROTOCOL_VERSION {
        return Err(EventDecodeError::UnsupportedVersion(version));
    }

    let kind = EventKind::from_u8(input[6])?;
    let flags = input[7];
    let sequence = u64::from_be_bytes(input[8..16].try_into().expect("fixed sequence slice"));
    let observed_at_unix_ns =
        u64::from_be_bytes(input[16..24].try_into().expect("fixed timestamp slice"));
    let chain_from = i32::from_be_bytes(input[24..28].try_into().expect("fixed chain slice"));
    let chain_to = i32::from_be_bytes(input[28..32].try_into().expect("fixed chain slice"));
    let mut transaction_id = [0u8; 32];
    transaction_id.copy_from_slice(&input[32..64]);
    let payload_len =
        u32::from_be_bytes(input[64..68].try_into().expect("fixed payload length slice")) as usize;
    let expected = EVENT_HEADER_LEN.saturating_add(payload_len);
    if input.len() < expected {
        return Err(EventDecodeError::Truncated {
            expected,
            actual: input.len(),
        });
    }
    if input.len() != expected {
        return Err(EventDecodeError::TrailingBytes {
            expected,
            actual: input.len(),
        });
    }

    Ok(BotEvent {
        kind,
        flags,
        sequence,
        observed_at_unix_ns,
        chain_from,
        chain_to,
        transaction_id,
        payload: input[EVENT_HEADER_LEN..].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::{BotEvent, EventKind, decode_event, encode_event};

    #[test]
    fn raw_transaction_event_round_trips() {
        let mut event = BotEvent::raw_transaction(1, 2, [7; 32], vec![1, 2, 3], 99);
        event.sequence = 42;
        let encoded = encode_event(&event).expect("encode");
        let decoded = decode_event(&encoded).expect("decode");
        assert_eq!(decoded, event);
        assert_eq!(decoded.kind, EventKind::RawTransaction);
    }
}
