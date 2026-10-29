//! Shadow mode: the client gets the official response; the native answer is
//! computed alongside and compared, so a route can go native only after it has
//! matched the official node on live traffic.

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
    let native = native::serve(route, &path, query.as_deref(), &state.native_context());

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

/// JSON equality ignoring formatting and key order, byte equality otherwise;
/// a transaction status also allows confirmation counts within
/// `CONFIRMATION_LAG` of the official ones.
fn same_answer(route: Route, official: &Bytes, native: &[u8]) -> bool {
    match (
        serde_json::from_slice::<serde_json::Value>(official),
        serde_json::from_slice::<serde_json::Value>(native),
    ) {
        (Ok(official), Ok(native)) if route == Route::TransactionStatus => {
            same_status(&official, &native)
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

    use super::{same_answer, same_status};
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
    }
}
