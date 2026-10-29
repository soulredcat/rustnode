use crate::{DecodeResult, Error, Staging, compact};

fn encode_len(length: usize) -> Result<Vec<u8>, Error> {
    let length = i32::try_from(length).map_err(|_| Error::LengthOutOfRange { length })?;
    Ok(compact::encode_i32(length))
}

fn decode_non_negative_len<'a>(input: &'a [u8], kind: &str) -> DecodeResult<'a, usize> {
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

pub fn decode_bytes<'a>(input: &'a [u8], max_len: usize) -> DecodeResult<'a, &'a [u8]> {
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

#[cfg(test)]
mod tests {
    use super::{decode_bytes, encode_bytes};
    use crate::Error;

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
}
