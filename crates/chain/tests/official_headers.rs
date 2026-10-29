//! Header validation against the operator's official mainnet node: a
//! checkpoint of recent heights is imported, then the newest
//! `RUSTNODE_VALIDATE_HEIGHTS` heights (default 30) of every chain are
//! validated and added in dependency order. Every mainnet block must pass,
//! the work target included. Canonical blocks must equal the official ones
//! from `SETTLED_DEPTH` heights below the newest: closer to the tip, which
//! fork sibling is canonical depends on the order blocks arrived, in the
//! official node as in any other.
//!
//! `RUSTNODE_OFFICIAL_URL=http://127.0.0.1:12973 [RUSTNODE_OFFICIAL_API_KEY=...]
//!  cargo test -p alephium-chain --test official_headers -- --ignored --nocapture`

use alephium_chain::{AddError, ChainStore};
use alephium_model::hash::from_hex;
use alephium_model::{Block, deserialize};
use serde_json::Value;

/// Heights below the newest added one where forks have resolved.
const SETTLED_DEPTH: u32 = 10;

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

    fn hashes(&self, from: u32, to: u32, height: u32) -> Vec<String> {
        self.get(&format!(
            "/blockflow/hashes?fromGroup={from}&toGroup={to}&height={height}"
        ))["headers"]
            .as_array()
            .expect("headers")
            .iter()
            .map(|hash| hash.as_str().expect("hash").to_owned())
            .collect()
    }

    fn block(&self, hash: &str) -> Block {
        let raw = self.get(&format!("/blockflow/raw-blocks/{hash}"));
        let bytes = from_hex(raw["value"].as_str().expect("raw")).expect("hex");
        deserialize(&bytes).expect("block")
    }
}

#[test]
#[ignore = "needs the official mainnet node"]
fn mainnet_headers_pass_validation() {
    let node = Node {
        url: std::env::var("RUSTNODE_OFFICIAL_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:12973".into()),
        key: std::env::var("RUSTNODE_OFFICIAL_API_KEY").ok(),
    };
    let validated: u32 = std::env::var("RUSTNODE_VALIDATE_HEIGHTS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(30);
    // The difficulty adjustment looks back 17 heights from tips that can
    // themselves trail; 120 heights of checkpoint cover it.
    let checkpoint = 120;
    let groups = 4;

    let mut genesis = Vec::new();
    let mut tops = Vec::new();
    for from in 0..groups {
        for to in 0..groups {
            genesis.push(node.block(&node.hashes(from, to, 0)[0]));
            let top = node.get(&format!(
                "/blockflow/chain-info?fromGroup={from}&toGroup={to}"
            ))["currentHeight"]
                .as_u64()
                .expect("height") as u32;
            tops.push((from, to, top));
        }
    }
    let mut store = ChainStore::new(groups, 100_000, genesis, None).expect("genesis");
    let mut pending = Vec::new();
    let mut official = Vec::new();
    for &(from, to, top) in &tops {
        for height in top - checkpoint..=top {
            let hashes = node.hashes(from, to, height);
            let blocks: Vec<Block> = hashes.iter().map(|hash| node.block(hash)).collect();
            if height + validated <= top {
                store
                    .import_checkpoint(from, to, height, blocks)
                    .expect("import");
            } else {
                official.push((from, to, height, hashes));
                pending.extend(blocks);
            }
        }
    }
    store.finish_checkpoint().expect("finish");

    // Add in dependency order, as the blocks would arrive.
    pending.sort_by_key(|block| block.header.timestamp);
    let total = pending.len();
    while !pending.is_empty() {
        let before = pending.len();
        let mut retry = Vec::new();
        for block in pending {
            match store
                .validate(&block)
                .and_then(|()| store.add(block.clone()))
            {
                Ok(_) => {}
                Err(AddError::MissingDeps(_)) => retry.push(block),
                Err(error) => panic!("{} rejected: {error:?}", block.hash().to_hex()),
            }
        }
        assert!(
            retry.len() < before,
            "no progress with {} blocks",
            retry.len()
        );
        pending = retry;
    }
    let mut settled = 0;
    for (from, to, height, _) in official {
        let top = store.max_height(from, to);
        if height + SETTLED_DEPTH > top {
            continue;
        }
        let ours = store
            .hashes_at(from, to, height)
            .first()
            .map(|hash| hash.to_hex());
        let theirs = node.hashes(from, to, height).first().cloned();
        assert_eq!(
            ours,
            theirs,
            "canonical {from}->{to} h{height}, {} below the newest",
            top - height
        );
        settled += 1;
    }
    println!("{settled} settled heights on the official canonical chain");
    println!("{total} mainnet blocks validated, work targets included");
}
