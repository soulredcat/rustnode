//! Shadow mode: the client gets the official response; the native answer is
//! computed alongside and compared, so a route can go native only after it has
//! matched the official node on live traffic.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use http_body_util::{BodyExt, Limited};
use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode};

use crate::metrics::ShadowResult;
use crate::proxy::{self, Forwarded};
use crate::routes::Route;
use crate::{State, full_body, native};

/// Official responses for shadowed routes are buffered for comparison.
const MAX_SHADOW_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
/// Mismatch details are logged for the first few per route, then only counted.
const MAX_LOGGED_MISMATCHES: u64 = 20;
const LOGGED_BODY_CHARS: usize = 300;

pub(crate) async fn forward_and_compare(
    state: &State,
    route: Route,
    request: Request<Incoming>,
) -> Forwarded {
    let path = request.uri().path().to_owned();
    let query = request.uri().query().map(str::to_owned);

    let upstream = match proxy::send(state, request).await {
        Ok(response) => response,
        Err(failure) => return failure.into(),
    };
    let (parts, body) = upstream.into_parts();
    let Ok(collected) = Limited::new(body, MAX_SHADOW_RESPONSE_BYTES)
        .collect()
        .await
    else {
        return proxy::upstream_failure(
            StatusCode::BAD_GATEWAY,
            "official node response could not be read",
        )
        .into();
    };
    let official = collected.to_bytes();
    // Taken after the official answer: rustnode hears of blocks and
    // transactions from the official node, so it is a moment behind it.
    let native = native::serve(route, &path, query.as_deref(), &state.native_context());

    let result = match &native {
        None => ShadowResult::Declined,
        Some(native) if parts.status == StatusCode::OK && same_answer(route, &official, native) => {
            ShadowResult::Match
        }
        Some(_) => ShadowResult::Mismatch,
    };
    let earlier_mismatches = state.metrics.record_shadow(route, result);
    if result == ShadowResult::Mismatch && earlier_mismatches < MAX_LOGGED_MISMATCHES {
        eprintln!(
            "api shadow mismatch {} {path}: official {} {} | native {}",
            route.name(),
            parts.status,
            excerpt(&official),
            native.as_deref().map(excerpt).unwrap_or_default()
        );
    }

    Forwarded {
        response: Response::from_parts(parts, full_body(official)),
        upstream_error: false,
    }
}

/// Confirmations may differ by this much: rustnode hears of a block from its
/// peer a moment after the official node, as any two nodes do.
const CONFIRMATION_LAG: i64 = 2;

/// The official node relays transactions in batches
/// (`txs-broadcast-delay`), so one it saw less than this long ago may not
/// have reached rustnode yet.
const RELAY_LAG_MILLIS: u64 = 15_000;

/// JSON equality ignoring formatting and key order, byte equality otherwise;
/// a transaction status also allows confirmation counts within
/// `CONFIRMATION_LAG` of the official ones, and the mempool is compared by
/// `same_mempool`.
fn same_answer(route: Route, official: &Bytes, native: &[u8]) -> bool {
    match (
        serde_json::from_slice::<serde_json::Value>(official),
        serde_json::from_slice::<serde_json::Value>(native),
    ) {
        (Ok(official), Ok(native)) if route == Route::TransactionStatus => {
            same_status(&official, &native)
        }
        (Ok(official), Ok(native)) if route == Route::MempoolTransactions => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| {
                    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
                });
            same_mempool(&official, &native, now)
        }
        (Ok(mut official), Ok(mut native)) if route == Route::AddressBalance => {
            // The official token lists follow a hash map's order.
            for body in [&mut official, &mut native] {
                for key in ["tokenBalances", "lockedTokenBalances"] {
                    if let Some(tokens) =
                        body.get_mut(key).and_then(serde_json::Value::as_array_mut)
                    {
                        tokens.sort_by_key(ToString::to_string);
                    }
                }
            }
            official == native
        }
        (Ok(official), Ok(native)) => official == native,
        _ => official.as_ref() == native,
    }
}

fn same_status(official: &serde_json::Value, native: &serde_json::Value) -> bool {
    const COUNTS: [&str; 3] = [
        "chainConfirmations",
        "fromGroupConfirmations",
        "toGroupConfirmations",
    ];
    // Confirmed within the lag: rustnode may not have the block yet.
    if official["type"] == "Confirmed"
        && official["chainConfirmations"]
            .as_i64()
            .is_some_and(|count| count <= CONFIRMATION_LAG)
        && (native["type"] == "MemPooled" || native["type"] == "TxNotFound")
    {
        return true;
    }
    let (Some(official_fields), Some(native_fields)) = (official.as_object(), native.as_object())
    else {
        return official == native;
    };
    official_fields.len() == native_fields.len()
        && official_fields.iter().all(|(key, value)| {
            let Some(ours) = native_fields.get(key) else {
                return false;
            };
            match (
                COUNTS.contains(&key.as_str()),
                value.as_i64(),
                ours.as_i64(),
            ) {
                (true, Some(theirs), Some(ours)) => (theirs - ours).abs() <= CONFIRMATION_LAG,
                _ => value == ours,
            }
        })
}

/// The same transactions on the same chain indexes, in any order and with
/// any `seenAt`, except that the official node may hold some it saw within
/// `RELAY_LAG_MILLIS` of `now`.
fn same_mempool(official: &serde_json::Value, native: &serde_json::Value, now: u64) -> bool {
    let (Some(official), Some(native)) = (mempool_txs(official), mempool_txs(native)) else {
        return false;
    };
    native
        .iter()
        .all(|(id, (tx, _))| official.get(id).is_some_and(|(theirs, _)| theirs == tx))
        && official.iter().all(|(id, (_, seen_at))| {
            native.contains_key(id) || now.saturating_sub(*seen_at) < RELAY_LAG_MILLIS
        })
}

/// A mempool answer's transactions by id, each with its chain index in place
/// of `seenAt`, and that `seenAt`; `None` for any other shape.
fn mempool_txs(body: &serde_json::Value) -> Option<HashMap<String, (serde_json::Value, u64)>> {
    let mut txs = HashMap::new();
    for chain in body.as_array()? {
        let (from, to) = (chain.get("fromGroup")?, chain.get("toGroup")?);
        for tx in chain.get("transactions")?.as_array()? {
            let mut tx = tx.as_object()?.clone();
            let seen_at = tx.remove("seenAt")?.as_u64()?;
            let id = tx.get("unsigned")?.get("txId")?.as_str()?.to_owned();
            tx.insert("fromGroup".to_owned(), from.clone());
            tx.insert("toGroup".to_owned(), to.clone());
            if txs
                .insert(id, (serde_json::Value::Object(tx), seen_at))
                .is_some()
            {
                return None;
            }
        }
    }
    Some(txs)
}

fn excerpt(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .take(LOGGED_BODY_CHARS)
        .collect()
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use serde_json::json;

    use super::{RELAY_LAG_MILLIS, same_answer, same_mempool, same_status};
    use crate::routes::Route;

    #[test]
    fn json_comparison_ignores_formatting_and_key_order() {
        assert!(same_answer(
            Route::AddressGroup,
            &Bytes::from_static(b"{\"group\": 2}"),
            b"{\"group\":2}"
        ));
        assert!(same_answer(
            Route::AddressGroup,
            &Bytes::from_static(b"{\"a\":1,\"b\":[1,2]}"),
            b"{\"b\":[1,2],\"a\":1}"
        ));
        assert!(!same_answer(
            Route::AddressGroup,
            &Bytes::from_static(b"{\"group\":2}"),
            b"{\"group\":3}"
        ));
        assert!(!same_answer(
            Route::AddressGroup,
            &Bytes::from_static(b"not json"),
            b"{\"group\":2}"
        ));
    }

    #[test]
    fn statuses_match_within_the_confirmation_lag_only() {
        let status = |block: &str, confirmations: i64| {
            json!({"type": "Confirmed", "blockHash": block, "txIndex": 0,
                "chainConfirmations": confirmations, "fromGroupConfirmations": confirmations,
                "toGroupConfirmations": confirmations})
        };
        assert!(same_status(&status("aa", 10), &status("aa", 9)));
        assert!(same_status(&status("aa", 10), &status("aa", 12)));
        assert!(!same_status(&status("aa", 10), &status("aa", 7)));
        assert!(!same_status(&status("aa", 10), &status("bb", 10)));
        assert!(!same_status(
            &json!({"type": "MemPooled"}),
            &status("aa", 1)
        ));
        // A block rustnode may not have yet, and one it must have.
        assert!(same_status(&status("aa", 2), &json!({"type": "MemPooled"})));
        assert!(same_status(
            &status("aa", 1),
            &json!({"type": "TxNotFound"})
        ));
        assert!(!same_status(
            &status("aa", 3),
            &json!({"type": "MemPooled"})
        ));
        assert!(!same_status(
            &json!({"type": "MemPooled"}),
            &json!({"type": "TxNotFound"})
        ));
    }

    #[test]
    fn mempools_match_in_any_order_and_within_the_relay_lag_only() {
        let tx = |id: &str, seen_at: u64| {
            json!({"unsigned": {"txId": id, "gasAmount": 20000}, "inputSignatures": [],
                "scriptSignatures": [], "seenAt": seen_at})
        };
        let chain = |from: u32, txs: Vec<serde_json::Value>| json!({"fromGroup": from, "toGroup": 2, "transactions": txs});
        let now = 100_000_000;
        let official = json!([
            chain(0, vec![tx("aa", 1_000), tx("bb", 2_000)]),
            chain(1, vec![tx("cc", 3)])
        ]);
        // Any order, any seenAt.
        let native = json!([
            chain(1, vec![tx("cc", 9)]),
            chain(0, vec![tx("bb", 7), tx("aa", 8)])
        ]);
        assert!(same_mempool(&official, &native, now));
        // Missing one the official node saw long ago.
        let native = json!([chain(0, vec![tx("aa", 8), tx("bb", 7)])]);
        assert!(!same_mempool(&official, &native, now));
        // Missing one it has only just seen.
        let official = json!([chain(
            0,
            vec![tx("aa", 1_000), tx("bb", now - RELAY_LAG_MILLIS + 1)]
        )]);
        let native = json!([chain(0, vec![tx("aa", 8)])]);
        assert!(same_mempool(&official, &native, now));
        // Holding one the official node does not, or on another chain index.
        let native = json!([chain(0, vec![tx("aa", 8), tx("bb", 7), tx("dd", 1)])]);
        assert!(!same_mempool(&official, &native, now));
        let native = json!([chain(1, vec![tx("aa", 8), tx("bb", 7)])]);
        assert!(!same_mempool(&official, &native, now));
        assert!(same_mempool(&json!([]), &json!([]), now));
        assert!(!same_mempool(&json!([]), &json!({}), now));
    }
}
