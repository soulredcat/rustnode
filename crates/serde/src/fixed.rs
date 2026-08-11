use crate::{DecodeResult, Error, Staging};

pub const I32_BYTES: usize = core::mem::size_of::<i32>();
pub const I64_BYTES: usize = core::mem::size_of::<i64>();

#[must_use]
pub const fn encode_i32_be(value: i32) -> [u8; I32_BYTES] {
    value.to_be_bytes()
}

#[must_use]
pub const fn encode_i64_be(value: i64) -> [u8; I64_BYTES] {
    value.to_be_bytes()
}

pub fn take(input: &[u8], expected: usize) -> DecodeResult<'_, &[u8]> {
    if input.len() < expected {
        Err(Error::not_enough_bytes(expected, input.len()))
    } else {
        let (value, rest) = input.split_at(expected);
        Ok(Staging::new(value, rest))
    }
}

pub fn decode_array<const N: usize>(input: &[u8]) -> DecodeResult<'_, [u8; N]> {
    let staged = take(input, N)?;
    let value = <[u8; N]>::try_from(staged.value)
        .map_err(|_| Error::wrong_format("internal fixed-size conversion mismatch"))?;
    Ok(Staging::new(value, staged.rest))
}

pub fn decode_i32_be(input: &[u8]) -> DecodeResult<'_, i32> {
    decode_array::<I32_BYTES>(input).map(|staged| staged.map(i32::from_be_bytes))
}

pub fn decode_i64_be(input: &[u8]) -> DecodeResult<'_, i64> {
    decode_array::<I64_BYTES>(input).map(|staged| staged.map(i64::from_be_bytes))
}

#[cfg(test)]
mod tests {
    use super::{decode_array, decode_i32_be, decode_i64_be, encode_i32_be, encode_i64_be};
    use crate::Error;

    #[test]
    fn fixed_integers_are_big_endian_and_preserve_rest() {
        assert_eq!(encode_i32_be(0x0102_0304), [1, 2, 3, 4]);
        assert_eq!(
            encode_i64_be(0x0102_0304_0506_0708),
            [1, 2, 3, 4, 5, 6, 7, 8]
        );

        let i32_value = decode_i32_be(&[1, 2, 3, 4, 0xaa]).expect("valid i32");
        assert_eq!(i32_value.value, 0x0102_0304);
        assert_eq!(i32_value.rest, &[0xaa]);

        let i64_value =
            decode_i64_be(&[1, 2, 3, 4, 5, 6, 7, 8, 0xbb]).expect("valid i64");
        assert_eq!(i64_value.value, 0x0102_0304_0506_0708);
        assert_eq!(i64_value.rest, &[0xbb]);
    }

    #[test]
    fn fixed_values_reject_truncated_input() {
        assert_eq!(
            decode_array::<4>(&[1, 2, 3]),
            Err(Error::NotEnoughBytes {
                expected: 4,
                actual: 3,
            })
        );
    }
}
