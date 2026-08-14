use alephium_serde::{DecodeResult, Error, Staging, compact};

use super::payload::{RequestId, TxsResponse};

/// Decode the subset of `TxsResponse` intentionally used by the bot fast path.
///
/// The official wire format serializes a vector of `TransactionTemplate` values
/// without per-item byte lengths. Until this repository owns a canonical typed
/// transaction parser, a response containing more than one transaction cannot be
/// split safely. The fast path therefore issues one transaction ID per request and
/// rejects responses declaring more than one item.
pub fn decode_single_txs_response_payload(input: &[u8]) -> DecodeResult<'_, TxsResponse> {
    let request_id = compact::decode_u32(input)?;
    let count = compact::decode_i32(request_id.rest)?;

    match count.value {
        0 if count.rest.is_empty() => Ok(Staging::new(
            TxsResponse {
                id: RequestId {
                    value: request_id.value,
                },
                txs: Vec::new(),
            },
            &[],
        )),
        0 => Err(Error::validation(
            "empty TxsResponse contains trailing transaction bytes".to_owned(),
        )),
        1 if count.rest.is_empty() => Err(Error::validation(
            "single-item TxsResponse contains no transaction bytes".to_owned(),
        )),
        1 => Ok(Staging::new(
            TxsResponse {
                id: RequestId {
                    value: request_id.value,
                },
                txs: vec![count.rest.to_vec()],
            },
            &[],
        )),
        declared if declared < 0 => Err(Error::validation(format!(
            "negative TxsResponse transaction count: {declared}"
        ))),
        declared => Err(Error::validation(format!(
            "bot fast path requires one transaction per TxsResponse, peer declared {declared}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use alephium_serde::compact;

    use super::decode_single_txs_response_payload;

    #[test]
    fn decodes_one_raw_transaction() {
        let mut payload = compact::encode_u32(7);
        payload.extend_from_slice(&compact::encode_i32(1));
        payload.extend_from_slice(&[1, 2, 3]);

        let decoded = decode_single_txs_response_payload(&payload)
            .expect("decode")
            .finish(payload.len())
            .expect("finish");
        assert_eq!(decoded.id.value, 7);
        assert_eq!(decoded.txs, vec![vec![1, 2, 3]]);
    }

    #[test]
    fn rejects_multiple_transactions_without_typed_boundaries() {
        let mut payload = compact::encode_u32(7);
        payload.extend_from_slice(&compact::encode_i32(2));
        payload.extend_from_slice(&[1, 2, 3]);

        assert!(decode_single_txs_response_payload(&payload).is_err());
    }
}
