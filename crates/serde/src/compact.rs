use crate::{DecodeResult, Error, Staging};

const MODE_MASK: u8 = 0xc0;
const VALUE_MASK: u8 = 0x3f;
const SIGN_FLAG: u8 = 0x20;

const SINGLE_PREFIX: u8 = 0x00;
const TWO_PREFIX: u8 = 0x40;
const FOUR_PREFIX: u8 = 0x80;
const MULTI_PREFIX: u8 = 0xc0;

const SINGLE_NEGATIVE_PREFIX: i32 = 0xc0;
const TWO_NEGATIVE_PREFIX: i32 = 0x80;
const FOUR_NEGATIVE_PREFIX: i32 = 0x40;

const UNSIGNED_ONE_BOUND: u32 = 0x40;
const UNSIGNED_TWO_BOUND: u32 = UNSIGNED_ONE_BOUND << 8;
const UNSIGNED_FOUR_BOUND: u32 = UNSIGNED_ONE_BOUND << 24;

const SIGNED_ONE_BOUND: i32 = 0x20;
const SIGNED_TWO_BOUND: i32 = SIGNED_ONE_BOUND << 8;
const SIGNED_FOUR_BOUND: i32 = SIGNED_ONE_BOUND << 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixedMode {
    Single,
    Two,
    Four,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Fixed(FixedMode),
    Multi,
}

fn split_mode(input: &[u8]) -> Result<(Mode, &[u8], &[u8]), Error> {
    let Some(first) = input.first().copied() else {
        return Err(Error::not_enough_bytes(1, 0));
    };

    let (mode, expected) = match first & MODE_MASK {
        SINGLE_PREFIX => (Mode::Fixed(FixedMode::Single), 1),
        TWO_PREFIX => (Mode::Fixed(FixedMode::Two), 2),
        FOUR_PREFIX => (Mode::Fixed(FixedMode::Four), 4),
        _ => (Mode::Multi, usize::from(first & VALUE_MASK) + 5),
    };

    if input.len() < expected {
        Err(Error::not_enough_bytes(expected, input.len()))
    } else {
        let (body, rest) = input.split_at(expected);
        Ok((mode, body, rest))
    }
}

#[must_use]
pub fn encode_u32(value: u32) -> Vec<u8> {
    if value < UNSIGNED_ONE_BOUND {
        vec![value as u8]
    } else if value < UNSIGNED_TWO_BOUND {
        vec![((value >> 8) as u8).wrapping_add(TWO_PREFIX), value as u8]
    } else if value < UNSIGNED_FOUR_BOUND {
        vec![
            ((value >> 24) as u8).wrapping_add(FOUR_PREFIX),
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ]
    } else {
        let mut encoded = Vec::with_capacity(5);
        encoded.push(MULTI_PREFIX);
        encoded.extend_from_slice(&value.to_be_bytes());
        encoded
    }
}

pub fn decode_u32(input: &[u8]) -> DecodeResult<'_, u32> {
    let (mode, body, rest) = split_mode(input)?;
    let value = match mode {
        Mode::Fixed(FixedMode::Single) => u32::from(body[0]),
        Mode::Fixed(FixedMode::Two) => {
            (u32::from(body[0] & VALUE_MASK) << 8) | u32::from(body[1])
        }
        Mode::Fixed(FixedMode::Four) => {
            (u32::from(body[0] & VALUE_MASK) << 24)
                | (u32::from(body[1]) << 16)
                | (u32::from(body[2]) << 8)
                | u32::from(body[3])
        }
        Mode::Multi => {
            if body.len() != 5 {
                return Err(Error::wrong_format(format!(
                    "expect 4 bytes int, but got {} bytes int",
                    body.len() - 1
                )));
            }
            u32::from_be_bytes([body[1], body[2], body[3], body[4]])
        }
    };

    Ok(Staging::new(value, rest))
}

#[must_use]
pub fn encode_i32(value: i32) -> Vec<u8> {
    if value >= 0 {
        encode_positive_i32(value)
    } else {
        encode_negative_i32(value)
    }
}

fn encode_positive_i32(value: i32) -> Vec<u8> {
    if value < SIGNED_ONE_BOUND {
        vec![value as u8]
    } else if value < SIGNED_TWO_BOUND {
        vec![((value >> 8) as u8).wrapping_add(TWO_PREFIX), value as u8]
    } else if value < SIGNED_FOUR_BOUND {
        vec![
            ((value >> 24) as u8).wrapping_add(FOUR_PREFIX),
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ]
    } else {
        let mut encoded = Vec::with_capacity(5);
        encoded.push(MULTI_PREFIX);
        encoded.extend_from_slice(&value.to_be_bytes());
        encoded
    }
}

fn encode_negative_i32(value: i32) -> Vec<u8> {
    if value >= -SIGNED_ONE_BOUND {
        vec![(value ^ SINGLE_NEGATIVE_PREFIX) as u8]
    } else if value >= -SIGNED_TWO_BOUND {
        vec![
            ((value >> 8) ^ TWO_NEGATIVE_PREFIX) as u8,
            value as u8,
        ]
    } else if value >= -SIGNED_FOUR_BOUND {
        vec![
            ((value >> 24) ^ FOUR_NEGATIVE_PREFIX) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ]
    } else {
        let mut encoded = Vec::with_capacity(5);
        encoded.push(MULTI_PREFIX);
        encoded.extend_from_slice(&value.to_be_bytes());
        encoded
    }
}

pub fn decode_i32(input: &[u8]) -> DecodeResult<'_, i32> {
    let (mode, body, rest) = split_mode(input)?;
    let value = match mode {
        Mode::Multi => {
            if body.len() != 5 {
                return Err(Error::wrong_format(format!(
                    "expect 4 bytes int, but got {} bytes int",
                    body.len() - 1
                )));
            }
            i32::from_be_bytes([body[1], body[2], body[3], body[4]])
        }
        Mode::Fixed(fixed_mode) => decode_fixed_i32(fixed_mode, body),
    };

    Ok(Staging::new(value, rest))
}

fn decode_fixed_i32(mode: FixedMode, body: &[u8]) -> i32 {
    if body[0] & SIGN_FLAG == 0 {
        decode_positive_i32(mode, body)
    } else {
        decode_negative_i32(mode, body)
    }
}

fn decode_positive_i32(mode: FixedMode, body: &[u8]) -> i32 {
    match mode {
        FixedMode::Single => i32::from(body[0]),
        FixedMode::Two => {
            (i32::from(body[0] & VALUE_MASK) << 8) | i32::from(body[1])
        }
        FixedMode::Four => {
            (i32::from(body[0] & VALUE_MASK) << 24)
                | (i32::from(body[1]) << 16)
                | (i32::from(body[2]) << 8)
                | i32::from(body[3])
        }
    }
}

fn decode_negative_i32(mode: FixedMode, body: &[u8]) -> i32 {
    const NEGATIVE_MODE_MASK: i32 = !0x3f;

    match mode {
        FixedMode::Single => i32::from(body[0]) | NEGATIVE_MODE_MASK,
        FixedMode::Two => {
            ((i32::from(body[0]) | NEGATIVE_MODE_MASK) << 8) | i32::from(body[1])
        }
        FixedMode::Four => {
            ((i32::from(body[0]) | NEGATIVE_MODE_MASK) << 24)
                | (i32::from(body[1]) << 16)
                | (i32::from(body[2]) << 8)
                | i32::from(body[3])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_i32, decode_u32};
    use crate::Error;

    #[test]
    fn accepts_official_non_canonical_signed_encodings() {
        for bytes in [
            &[0x40, 0x00][..],
            &[0x80, 0x00, 0x00, 0x00][..],
            &[0xc0, 0x00, 0x00, 0x00, 0x00][..],
        ] {
            let decoded = decode_i32(bytes).expect("official non-canonical zero must decode");
            assert_eq!(decoded.value, 0);
            assert!(decoded.rest.is_empty());
        }

        for bytes in [
            &[0x7f, 0xff][..],
            &[0xbf, 0xff, 0xff, 0xff][..],
            &[0xc0, 0xff, 0xff, 0xff, 0xff][..],
        ] {
            let decoded = decode_i32(bytes).expect("official non-canonical -1 must decode");
            assert_eq!(decoded.value, -1);
            assert!(decoded.rest.is_empty());
        }
    }

    #[test]
    fn accepts_official_non_canonical_unsigned_encodings() {
        for bytes in [
            &[0x40, 0x00][..],
            &[0x80, 0x00, 0x00, 0x00][..],
            &[0xc0, 0x00, 0x00, 0x00, 0x00][..],
        ] {
            let decoded = decode_u32(bytes).expect("official non-canonical zero must decode");
            assert_eq!(decoded.value, 0);
            assert!(decoded.rest.is_empty());
        }
    }

    #[test]
    fn rejects_multi_byte_widths_that_are_not_u32_or_i32() {
        assert!(matches!(
            decode_i32(&[0xc1, 0, 0, 0, 0, 0]),
            Err(Error::WrongFormat { .. })
        ));
        assert!(matches!(
            decode_u32(&[0xc1, 0, 0, 0, 0, 0]),
            Err(Error::WrongFormat { .. })
        ));
    }
}
