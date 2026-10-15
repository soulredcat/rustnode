//! Differential check of block JSON against a running official node: every
//! block is fetched raw, decoded, rendered with `alephium_api::json`, and
//! compared with the official `GET /blockflow/blocks/{hash}`.
//!
//! `RUSTNODE_OFFICIAL_URL=http://127.0.0.1:12973 [RUSTNODE_OFFICIAL_API_KEY=...]
//!  cargo test -p alephium-api --test official_block_json -- --ignored --nocapture`

use alephium_api::json::block_entry;
use alephium_model::hash::from_hex;
use alephium_model::network::Network;
use alephium_model::{Block, deserialize};
use serde_json::Value;

fn get(url: &str, api_key: Option<&str>, path: &str) -> Value {
    let mut request = ureq::get(&format!("{url}{path}"));
    if let Some(key) = api_key {
        request = request.set("X-API-KEY", key);
    }
    let body = request
        .call()
        .unwrap_or_else(|error| panic!("GET {path}: {error}"))
        .into_string()
        .expect("body");
    serde_json::from_str(&body).expect("json")
}

/// The first differing path, for a readable failure.
fn first_difference(path: &str, ours: &Value, official: &Value) -> Option<String> {
    match (ours, official) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys().chain(b.keys()) {
                let (x, y) = (
                    a.get(key).unwrap_or(&Value::Null),
                    b.get(key).unwrap_or(&Value::Null),
                );
                if let Some(found) = first_difference(&format!("{path}.{key}"), x, y) {
                    return Some(found);
                }
            }
            None
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => a
            .iter()
            .zip(b)
            .enumerate()
            .find_map(|(i, (x, y))| first_difference(&format!("{path}[{i}]"), x, y)),
        _ if ours == official => None,
        _ => Some(format!("{path}: ours {ours} | official {official}")),
    }
}

#[test]
#[ignore = "needs a running official node"]
fn every_official_block_renders_the_same_json() {
    let url =
        std::env::var("RUSTNODE_OFFICIAL_URL").unwrap_or_else(|_| "http://127.0.0.1:12973".into());
    let key = std::env::var("RUSTNODE_OFFICIAL_API_KEY").ok();
    let key = key.as_deref();
    let params = get(&url, key, "/infos/chain-params");
    let groups = params["groups"].as_u64().expect("groups") as u32;
    let network = Network::from_id(params["networkId"].as_u64().expect("networkId") as u8);
    let max_blocks: u64 = std::env::var("RUSTNODE_MAX_BLOCKS_PER_CHAIN")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(u64::MAX);

    let mut checked = 0usize;
    for from in 0..groups {
        for to in 0..groups {
            let height = get(
                &url,
                key,
                &format!("/blockflow/chain-info?fromGroup={from}&toGroup={to}"),
            )["currentHeight"]
                .as_u64()
                .expect("height");
            for h in height.saturating_sub(max_blocks.saturating_sub(1))..=height {
                let hashes = get(
                    &url,
                    key,
                    &format!("/blockflow/hashes?fromGroup={from}&toGroup={to}&height={h}"),
                );
                for hash in hashes["headers"].as_array().expect("headers") {
                    let hash = hash.as_str().expect("hash");
                    let raw = get(&url, key, &format!("/blockflow/raw-blocks/{hash}"));
                    let block: Block =
                        deserialize(&from_hex(raw["value"].as_str().expect("raw")).expect("hex"))
                            .expect("decode");
                    let official = get(&url, key, &format!("/blockflow/blocks/{hash}"));
                    let conflicted: Option<Vec<_>> = official.get("conflictedTxs").map(|list| {
                        list.as_array()
                            .expect("conflictedTxs")
                            .iter()
                            .map(|id| {
                                alephium_model::Hash::from_hex(id.as_str().expect("id"))
                                    .expect("hash")
                            })
                            .collect()
                    });
                    let ours =
                        block_entry(&block, h as u32, groups, &network, conflicted.as_deref())
                            .unwrap_or_else(|| panic!("no JSON for {hash}"));
                    if let Some(difference) = first_difference("block", &ours, &official) {
                        panic!("{hash} ({from}->{to} h{h}) differs at {difference}");
                    }
                    assert_eq!(ours, official);
                    checked += 1;
                }
            }
        }
    }
    println!("{checked} blocks render the official JSON");
    assert!(checked > 0);
}
