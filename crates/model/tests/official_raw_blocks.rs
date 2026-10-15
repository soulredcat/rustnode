//! Differential check against a running official node: every block it holds is
//! fetched raw (`GET /blockflow/raw-blocks/{hash}`), decoded, re-encoded, and
//! hashed by this crate; bytes and hashes must match exactly.
//!
//! Run with an official node (any network):
//! `RUSTNODE_OFFICIAL_URL=http://127.0.0.1:12973 [RUSTNODE_OFFICIAL_API_KEY=...]
//!  cargo test -p alephium-model --test official_raw_blocks -- --ignored --nocapture`

use alephium_model::hash::from_hex;
use alephium_model::{Block, BlockHeader, Hash, deserialize, serialize};

struct Node {
    url: String,
    api_key: Option<String>,
}

impl Node {
    fn get(&self, path: &str) -> serde_json::Value {
        let mut request = ureq::get(&format!("{}{path}", self.url));
        if let Some(key) = &self.api_key {
            request = request.set("X-API-KEY", key);
        }
        request
            .call()
            .unwrap_or_else(|error| panic!("GET {path}: {error}"))
            .into_string()
            .map(|body| serde_json::from_str(&body).expect("json"))
            .expect("body")
    }
}

#[test]
#[ignore = "needs a running official node"]
fn every_official_block_round_trips() {
    let node = Node {
        url: std::env::var("RUSTNODE_OFFICIAL_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:12973".into()),
        api_key: std::env::var("RUSTNODE_OFFICIAL_API_KEY").ok(),
    };
    let groups = node.get("/infos/chain-params")["groups"]
        .as_u64()
        .expect("groups") as u32;
    let max_blocks: u64 = std::env::var("RUSTNODE_MAX_BLOCKS_PER_CHAIN")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(u64::MAX);

    let (mut blocks, mut transactions, mut scripts) = (0usize, 0usize, 0usize);
    for from in 0..groups {
        for to in 0..groups {
            let height = node.get(&format!(
                "/blockflow/chain-info?fromGroup={from}&toGroup={to}"
            ))["currentHeight"]
                .as_u64()
                .expect("height");
            let start = height.saturating_sub(max_blocks.saturating_sub(1));
            for h in start..=height {
                let hashes = node.get(&format!(
                    "/blockflow/hashes?fromGroup={from}&toGroup={to}&height={h}"
                ));
                for hash in hashes["headers"].as_array().expect("headers") {
                    let hash = hash.as_str().expect("hash");
                    let raw = node.get(&format!("/blockflow/raw-blocks/{hash}"));
                    let bytes = from_hex(raw["value"].as_str().expect("value")).expect("hex");
                    let block: Block = deserialize(&bytes).unwrap_or_else(|error| {
                        panic!("decode {hash} ({from}->{to} h{h}): {error}")
                    });
                    assert_eq!(serialize(&block), bytes, "re-encode {hash}");
                    assert_eq!(
                        block.hash(),
                        Hash::from_hex(hash).expect("hash"),
                        "hash {hash}"
                    );
                    if h > 0 {
                        assert_eq!(
                            BlockHeader::chain_index(&block.hash(), groups),
                            (from, to),
                            "chain index {hash}"
                        );
                    }
                    blocks += 1;
                    transactions += block.transactions.len();
                    scripts += block
                        .transactions
                        .iter()
                        .filter(|tx| tx.unsigned.script.is_some())
                        .count();
                }
            }
        }
    }
    println!("{blocks} blocks, {transactions} transactions ({scripts} with scripts) round-tripped");
    assert!(blocks > 0);
}
