//! Differential check of chain following against a running official node:
//! every block is imported in dependency order, then canonical hashes per
//! height, the height of the heaviest tip, and blocks by time range are
//! compared with the official answers.
//!
//! `RUSTNODE_OFFICIAL_URL=http://127.0.0.1:12973 [RUSTNODE_OFFICIAL_API_KEY=...]
//!  cargo test -p alephium-chain --test official_chain -- --ignored --nocapture`

use alephium_chain::{AddError, ChainStore};
use alephium_model::hash::from_hex;
use alephium_model::{Block, Hash, TimeStamp, deserialize};
use serde_json::Value;

struct Node {
    url: String,
    key: Option<String>,
}

impl Node {
    fn get(&self, path: &str) -> Value {
        let mut request = ureq::get(&format!("{}{path}", self.url));
        if let Some(key) = &self.key {
            request = request.set("X-API-KEY", key);
        }
        let body = request
            .call()
            .unwrap_or_else(|error| panic!("GET {path}: {error}"))
            .into_string()
            .expect("body");
        serde_json::from_str(&body).expect("json")
    }

    fn block(&self, hash: &str) -> Block {
        let raw = self.get(&format!("/blockflow/raw-blocks/{hash}"));
        deserialize(&from_hex(raw["value"].as_str().expect("raw")).expect("hex")).expect("decode")
    }
}

#[test]
#[ignore = "needs a running official node"]
fn chain_following_matches_the_official_node() {
    let node = Node {
        url: std::env::var("RUSTNODE_OFFICIAL_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:12973".into()),
        key: std::env::var("RUSTNODE_OFFICIAL_API_KEY").ok(),
    };
    let groups = node.get("/infos/chain-params")["groups"]
        .as_u64()
        .expect("groups") as u32;

    let mut genesis = Vec::new();
    let mut others = Vec::new();
    let mut official_hashes = Vec::new();
    for from in 0..groups {
        for to in 0..groups {
            let height = node.get(&format!(
                "/blockflow/chain-info?fromGroup={from}&toGroup={to}"
            ))["currentHeight"]
                .as_u64()
                .expect("height") as u32;
            for h in 0..=height {
                let hashes: Vec<String> = node.get(&format!(
                    "/blockflow/hashes?fromGroup={from}&toGroup={to}&height={h}"
                ))["headers"]
                    .as_array()
                    .expect("headers")
                    .iter()
                    .map(|hash| hash.as_str().expect("hash").to_owned())
                    .collect();
                for hash in &hashes {
                    let block = node.block(hash);
                    if h == 0 {
                        genesis.push(block)
                    } else {
                        others.push(block)
                    }
                }
                official_hashes.push(((from, to, h), hashes));
            }
        }
    }

    let mut store =
        ChainStore::new(groups, alephium_chain::DEFAULT_WINDOW, genesis).expect("genesis");
    // Import in dependency order, as blocks would arrive.
    others.sort_by_key(|block| block.header.timestamp);
    let mut pending = others;
    while !pending.is_empty() {
        let before = pending.len();
        let mut retry = Vec::new();
        for block in pending {
            match store.add(block.clone()) {
                Ok(_) => {}
                Err(AddError::MissingDeps(_)) => retry.push(block),
                Err(error) => panic!("add {}: {error:?}", block.hash().to_hex()),
            }
        }
        assert!(
            retry.len() < before,
            "no progress with {} blocks pending",
            retry.len()
        );
        pending = retry;
    }

    for ((from, to, h), hashes) in &official_hashes {
        let ours: Vec<String> = store
            .hashes_at(*from, *to, *h)
            .iter()
            .map(Hash::to_hex)
            .collect();
        assert_eq!(&ours, hashes, "hashes at {from}->{to} height {h}");
    }
    let mut chains = 0;
    for from in 0..groups {
        for to in 0..groups {
            let official = node.get(&format!(
                "/blockflow/chain-info?fromGroup={from}&toGroup={to}"
            ))["currentHeight"]
                .as_u64()
                .expect("height") as u32;
            assert_eq!(
                store.max_height_by_weight(from, to),
                official,
                "max height {from}->{to}"
            );
            chains += 1;
        }
    }

    // Blocks by time over the last 10 minutes, compared hash by hash.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis() as u64;
    let (from_ts, to_ts) = (now - 10 * 60 * 1000, now);
    let official = node.get(&format!("/blockflow/blocks?fromTs={from_ts}&toTs={to_ts}"));
    for (index, per_chain) in official["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .enumerate()
    {
        let (from, to) = (index as u32 / groups, index as u32 % groups);
        let official_hashes: Vec<&str> = per_chain
            .as_array()
            .expect("chain")
            .iter()
            .map(|block| block["hash"].as_str().expect("hash"))
            .collect();
        let ours: Vec<String> = store
            .blocks_between(from, to, TimeStamp(from_ts), TimeStamp(to_ts))
            .iter()
            .map(|(block, _)| block.hash().to_hex())
            .collect();
        assert_eq!(ours, official_hashes, "blocks by time {from}->{to}");
    }
    println!(
        "{} heights on {chains} chains match the official node",
        official_hashes.len()
    );
}
