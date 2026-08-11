use alephium_serde::{Error, compact, fixed};

/// Upstream parity sources:
/// - protocol/.../message/Message.scala
/// - protocol/.../message/MessageSerde.scala
/// - protocol/.../message/Header.scala
/// - protocol/.../Checksum.scala
/// - util/.../DjbHash.scala
///
/// Baseline: 63bcf6998508025f9538f3377629fe9e66df6c53.
pub const MAGIC_LENGTH: usize = 4;
pub const CHECKSUM_LENGTH: usize = 4;
pub const LENGTH_FIELD_LENGTH: usize = 4;
pub const ENVELOPE_LENGTH: usize = MAGIC_LENGTH + CHECKSUM_LENGTH + LENGTH_FIELD_LENGTH;

pub const CURRENT_WIRE_VERSION: i32 = 0x0001_0000;
pub const MAX_PAYLOAD_CODE: i32 = 20;

const DJB_INITIAL_HASH: i32 = 5381;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkMagic([u8; MAGIC_LENGTH]);

impl NetworkMagic {
    #[must_use]
    pub const fn new(bytes: [u8; MAGIC_LENGTH]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; MAGIC_LENGTH] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame<'a> {
    /// Checksum serialized in the frame. A decoded frame contains only a
    /// checksum already verified against `data`.
    pub checksum: i32,
    pub data: &'a [u8],
    pub consumed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDecode<'a> {
    NeedMoreData { required_total: usize },
    Complete(Frame<'a>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawMessageData<'a> {
    pub wire_version: i32,
    pub payload_code: i32,
    pub payload_body: &'a [u8],
}

/// Compute Alephium's four-byte message checksum.
///
/// Scala `Int` arithmetic wraps. Rust must therefore use explicit wrapping
/// operations in both debug and release builds.
#[must_use]
pub fn djb_checksum(data: &[u8]) -> i32 {
    data.iter().fold(DJB_INITIAL_HASH, |hash, byte| {
        hash.wrapping_mul(33).wrapping_add(i32::from(*byte))
    })
}

pub fn encode_frame(
    magic: NetworkMagic,
    data: &[u8],
    max_data_len: usize,
) -> Result<Vec<u8>, Error> {
    if data.len() > max_data_len {
        return Err(Error::LimitExceeded {
            declared: data.len(),
            limit: max_data_len,
        });
    }

    let data_len =
        i32::try_from(data.len()).map_err(|_| Error::LengthOutOfRange { length: data.len() })?;
    let capacity = ENVELOPE_LENGTH
        .checked_add(data.len())
        .ok_or(Error::LengthOutOfRange { length: data.len() })?;

    let mut encoded = Vec::with_capacity(capacity);
    encoded.extend_from_slice(magic.as_bytes());
    encoded.extend_from_slice(&fixed::encode_i32_be(djb_checksum(data)));
    encoded.extend_from_slice(&fixed::encode_i32_be(data_len));
    encoded.extend_from_slice(data);
    Ok(encoded)
}

pub fn decode_frame_prefix<'a>(
    input: &'a [u8],
    magic: NetworkMagic,
    max_data_len: usize,
) -> Result<FrameDecode<'a>, Error> {
    if input.len() < MAGIC_LENGTH {
        return Ok(FrameDecode::NeedMoreData {
            required_total: MAGIC_LENGTH,
        });
    }

    if input[..MAGIC_LENGTH] != magic.as_bytes()[..] {
        return Err(Error::wrong_format("wrong magic bytes"));
    }

    if input.len() < ENVELOPE_LENGTH {
        return Ok(FrameDecode::NeedMoreData {
            required_total: ENVELOPE_LENGTH,
        });
    }

    let checksum = i32::from_be_bytes(
        input[MAGIC_LENGTH..MAGIC_LENGTH + CHECKSUM_LENGTH]
            .try_into()
            .map_err(|_| Error::wrong_format("invalid checksum field width"))?,
    );
    let length_offset = MAGIC_LENGTH + CHECKSUM_LENGTH;
    let data_len = i32::from_be_bytes(
        input[length_offset..ENVELOPE_LENGTH]
            .try_into()
            .map_err(|_| Error::wrong_format("invalid length field width"))?,
    );

    if data_len < 0 {
        return Err(Error::wrong_format(format!("negative length: {data_len}")));
    }

    let data_len = data_len as usize;
    if data_len > max_data_len {
        return Err(Error::LimitExceeded {
            declared: data_len,
            limit: max_data_len,
        });
    }

    let required_total = ENVELOPE_LENGTH
        .checked_add(data_len)
        .ok_or(Error::LengthOutOfRange { length: data_len })?;
    if input.len() < required_total {
        return Ok(FrameDecode::NeedMoreData { required_total });
    }

    let data = &input[ENVELOPE_LENGTH..required_total];
    let expected_checksum = djb_checksum(data);
    if checksum != expected_checksum {
        return Err(Error::wrong_format(format!(
            "wrong checksum: expected {:08x}, got {:08x}",
            expected_checksum as u32, checksum as u32
        )));
    }

    Ok(FrameDecode::Complete(Frame {
        checksum,
        data,
        consumed: required_total,
    }))
}

pub fn decode_frame_exact<'a>(
    input: &'a [u8],
    magic: NetworkMagic,
    max_data_len: usize,
) -> Result<Frame<'a>, Error> {
    match decode_frame_prefix(input, magic, max_data_len)? {
        FrameDecode::NeedMoreData { required_total } => {
            Err(Error::not_enough_bytes(required_total, input.len()))
        }
        FrameDecode::Complete(frame) if frame.consumed == input.len() => Ok(frame),
        FrameDecode::Complete(frame) => Err(Error::redundant_bytes(frame.consumed, input.len())),
    }
}

pub fn encode_raw_message_data(payload_code: i32, payload_body: &[u8]) -> Result<Vec<u8>, Error> {
    validate_payload_code(payload_code)?;

    let wire_version = compact::encode_i32(CURRENT_WIRE_VERSION);
    let payload_code = compact::encode_i32(payload_code);
    let capacity = wire_version
        .len()
        .checked_add(payload_code.len())
        .and_then(|length| length.checked_add(payload_body.len()))
        .ok_or(Error::LengthOutOfRange {
            length: payload_body.len(),
        })?;

    let mut encoded = Vec::with_capacity(capacity);
    encoded.extend_from_slice(&wire_version);
    encoded.extend_from_slice(&payload_code);
    encoded.extend_from_slice(payload_body);
    Ok(encoded)
}

pub fn decode_raw_message_data(input: &[u8]) -> Result<RawMessageData<'_>, Error> {
    let wire_version = compact::decode_i32(input)?;
    if wire_version.value != CURRENT_WIRE_VERSION {
        return Err(Error::validation(format!(
            "invalid wire version: {}",
            wire_version.value
        )));
    }

    let payload_code = compact::decode_i32(wire_version.rest)?;
    validate_payload_code(payload_code.value)?;

    Ok(RawMessageData {
        wire_version: wire_version.value,
        payload_code: payload_code.value,
        payload_body: payload_code.rest,
    })
}

fn validate_payload_code(payload_code: i32) -> Result<(), Error> {
    if (0..=MAX_PAYLOAD_CODE).contains(&payload_code) {
        Ok(())
    } else {
        Err(Error::wrong_format(format!(
            "invalid payload code {payload_code}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{CURRENT_WIRE_VERSION, MAX_PAYLOAD_CODE, decode_raw_message_data};
    use alephium_serde::{Error, compact};

    #[test]
    fn raw_message_rejects_wrong_wire_version() {
        let mut data = compact::encode_i32(0);
        data.extend_from_slice(&compact::encode_i32(0));
        assert!(matches!(
            decode_raw_message_data(&data),
            Err(Error::Validation { .. })
        ));
    }

    #[test]
    fn raw_message_rejects_payload_code_outside_official_table() {
        for code in [-1, MAX_PAYLOAD_CODE + 1, i32::MAX] {
            let mut data = compact::encode_i32(CURRENT_WIRE_VERSION);
            data.extend_from_slice(&compact::encode_i32(code));
            assert!(matches!(
                decode_raw_message_data(&data),
                Err(Error::WrongFormat { .. })
            ));
        }
    }
}
