use crate::{compact, DecodeResult, Error, Staging};

fn encode_len(length: usize) -> Result<Vec<u8>, Error> {
    let length = i32::try_from(length).map_err(|_| Error::LengthOutOfRange { length })?;
    Ok(compact::encode_i32(length))
}

fn decode_non_negative_len(input: &[u8], kind: &str) -> DecodeResult<'_, usize> {
    let staged = compact::decode_i32(input)?;
    if staged.value < 0 {
        return Err(Error::validation(format!(
            "negative {kind} length: {}",
            staged.value
        )));
    }

    Ok(staged.map(|value| value as usize))
}

pub fn encode_bytes(value: &[u8], max_len: usize) -> Result<Vec<u8>, Error> {
    if value.len() > max_len {
        return Err(Error::LimitExceeded {
            declared: value.len(),
            limit: max_len,
        });
    }

    let mut encoded = encode_len(value.len())?;
    encoded.extend_from_slice(value);
    Ok(encoded)
}

pub fn decode_bytes(input: &[u8], max_len: usize) -> DecodeResult<'_, &[u8]> {
    let staged_len = decode_non_negative_len(input, "byte string")?;
    let length = staged_len.value;

    if length > max_len {
        return Err(Error::LimitExceeded {
            declared: length,
            limit: max_len,
        });
    }
    if staged_len.rest.len() < length {
        return Err(Error::not_enough_bytes(length, staged_len.rest.len()));
    }

    let (value, rest) = staged_len.rest.split_at(length);
    Ok(Staging::new(value, rest))
}

/// Encode an M1 protocol string.
///
/// M1 client identifiers are ASCII. The pinned Scala serializer counts
/// `String.length` before appending UTF-8 bytes, so non-ASCII serialization is
/// intentionally rejected until dedicated upstream vectors define that edge.
pub fn encode_string(value: &str, max_len: usize) -> Result<Vec<u8>, Error> {
    if !value.is_ascii() {
        return Err(Error::validation(
            "non-ASCII M1 string serialization is not vector-locked",
        ));
    }
    encode_bytes(value.as_bytes(), max_len)
}

pub fn decode_string(input: &[u8], max_len: usize) -> DecodeResult<'_, String> {
    let staged = decode_bytes(input, max_len)?;
    let value = core::str::from_utf8(staged.value)
        .map_err(|_| Error::validation("invalid UTF-8 string"))?
        .to_owned();
    Ok(Staging::new(value, staged.rest))
}

pub fn encode_vec<T>(
    values: &[T],
    max_items: usize,
    mut encode_item: impl FnMut(&T, &mut Vec<u8>) -> Result<(), Error>,
) -> Result<Vec<u8>, Error> {
    if values.len() > max_items {
        return Err(Error::LimitExceeded {
            declared: values.len(),
            limit: max_items,
        });
    }

    let mut encoded = encode_len(values.len())?;
    for value in values {
        encode_item(value, &mut encoded)?;
    }
    Ok(encoded)
}

pub fn decode_vec<'a, T>(
    input: &'a [u8],
    max_items: usize,
    mut decode_item: impl FnMut(&'a [u8]) -> DecodeResult<'a, T>,
) -> DecodeResult<'a, Vec<T>> {
    let staged_len = decode_non_negative_len(input, "vector")?;
    let count = staged_len.value;

    if count > staged_len.rest.len() {
        return Err(Error::validation(format!(
            "malicious vector size: {count}"
        )));
    }
    if count > max_items {
        return Err(Error::LimitExceeded {
            declared: count,
            limit: max_items,
        });
    }

    let mut values = Vec::with_capacity(count);
    let mut rest = staged_len.rest;
    for _ in 0..count {
        let staged = decode_item(rest)?;
        values.push(staged.value);
        rest = staged.rest;
    }

    Ok(Staging::new(values, rest))
}

#[cfg(test)]
mod tests {
    use super::{
        decode_bytes, decode_string, decode_vec, encode_bytes, encode_string, encode_vec,
    };
    use crate::{Error, fixed};

    #[test]
    fn bounded_bytes_round_trip_and_preserve_rest() {
        let encoded = encode_bytes(b"alph", 4).expect("fixture must encode");
        let mut input = encoded;
        input.extend_from_slice(&[0xaa, 0xbb]);

        let decoded = decode_bytes(&input, 4).expect("fixture must decode");
        assert_eq!(decoded.value, b"alph");
        assert_eq!(decoded.rest, &[0xaa, 0xbb]);
    }

    #[test]
    fn bounded_bytes_reject_negative_excessive_and_truncated_lengths() {
        assert!(matches!(
            decode_bytes(&[0x3f], 256),
            Err(Error::Validation { .. })
        ));
        assert_eq!(
            decode_bytes(&[0x01, 0xaa], 0),
            Err(Error::LimitExceeded {
                declared: 1,
                limit: 0,
            })
        );
        assert_eq!(
            decode_bytes(&[0x02, 0xaa], 2),
            Err(Error::NotEnoughBytes {
                expected: 2,
                actual: 1,
            })
        );
    }

    #[test]
    fn bounded_string_validates_utf8() {
        let encoded = encode_string("alephium", 8).expect("fixture must encode");
        assert_eq!(
            decode_string(&encoded, 8).expect("fixture must decode").value,
            "alephium"
        );
        assert!(matches!(
            decode_string(&[0x01, 0xff], 8),
            Err(Error::Validation { .. })
        ));
        assert!(matches!(
            encode_string("é", 8),
            Err(Error::Validation { .. })
        ));
    }

    #[test]
    fn bounded_vector_composes_staged_decoders() {
        let values = [1_i32, -1, i32::MAX];
        let encoded = encode_vec(&values, 3, |value, output| {
            output.extend_from_slice(&fixed::encode_i32_be(*value));
            Ok(())
        })
        .expect("fixture must encode");

        let decoded = decode_vec(&encoded, 3, |input| fixed::decode_i32_be(input))
            .expect("fixture must decode");
        assert_eq!(decoded.value, values);
        assert!(decoded.rest.is_empty());
    }

    #[test]
    fn vector_rejects_negative_malicious_and_locally_excessive_counts() {
        assert!(matches!(
            decode_vec::<u8>(&[0x3f], 8, |input| {
                let staged = fixed::take(input, 1)?;
                Ok(staged.map(|bytes| bytes[0]))
            }),
            Err(Error::Validation { .. })
        ));

        assert!(matches!(
            decode_vec::<u8>(&[0x02, 0xaa], 8, |input| {
                let staged = fixed::take(input, 1)?;
                Ok(staged.map(|bytes| bytes[0]))
            }),
            Err(Error::Validation { .. })
        ));

        assert_eq!(
            decode_vec::<u8>(&[0x02, 0xaa, 0xbb], 1, |input| {
                let staged = fixed::take(input, 1)?;
                Ok(staged.map(|bytes| bytes[0]))
            }),
            Err(Error::LimitExceeded {
                declared: 2,
                limit: 1,
            })
        );
    }
}
