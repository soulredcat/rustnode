//! Differential check against the operator's official mainnet node: the
//! newest `RUSTNODE_MAX_BLOCKS_PER_CHAIN` heights (default 100) of every chain
//! are fetched raw (`GET /blockflow/raw-blocks/{hash}`), decoded, re-encoded,
//! and hashed by this crate; bytes and hashes must match exactly.
//!
//! `RUSTNODE_OFFICIAL_URL=http://127.0.0.1:12973 [RUSTNODE_OFFICIAL_API_KEY=...]
//!  cargo test -p alephium-model --test official_raw_blocks -- --ignored --nocapture`

use alephium_model::hash::from_hex;
use std::collections::BTreeMap;

use alephium_model::{
    Block, BlockHeader, Hash, LockupScript, TxOutput, UnlockScript, deserialize, serialize,
};

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
#[ignore = "needs the official mainnet node"]
fn official_blocks_round_trip() {
    let node = Node {
        url: std::env::var("RUSTNODE_OFFICIAL_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:12973".into()),
        api_key: std::env::var("RUSTNODE_OFFICIAL_API_KEY").ok(),
    };
    let params = node.get("/infos/chain-params");
    assert_eq!(
        params["networkId"].as_u64(),
        Some(u64::from(alephium_model::network::NETWORK_ID)),
        "not a mainnet node"
    );
    let groups = params["groups"].as_u64().expect("groups") as u32;
    let max_blocks: u64 = std::env::var("RUSTNODE_MAX_BLOCKS_PER_CHAIN")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100);

    let (mut blocks, mut transactions, mut scripts) = (0usize, 0usize, 0usize);
    let mut kinds = BTreeMap::<&'static str, usize>::new();
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
                    for tx in &block.transactions {
                        for input in &tx.unsigned.inputs {
                            *kinds.entry(unlock_kind(&input.unlock_script)).or_default() += 1;
                        }
                        let outputs = tx
                            .unsigned
                            .fixed_outputs
                            .iter()
                            .map(|o| (&o.lockup_script, !o.tokens.is_empty()));
                        for (lockup, with_tokens) in outputs {
                            *kinds.entry(lockup_kind(lockup)).or_default() += 1;
                            if with_tokens {
                                *kinds.entry("asset output with tokens").or_default() += 1;
                            }
                        }
                        for output in &tx.generated_outputs {
                            if let TxOutput::Contract(contract) = output {
                                *kinds.entry("contract output").or_default() += 1;
                                if !contract.tokens.is_empty() {
                                    *kinds.entry("contract output with tokens").or_default() += 1;
                                }
                            }
                        }
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
    println!("seen: {kinds:?}");
    assert!(blocks > 0);
}

fn unlock_kind(unlock: &UnlockScript) -> &'static str {
    match unlock {
        UnlockScript::P2pkh(_) => "unlock P2PKH",
        UnlockScript::P2mpkh(_) => "unlock P2MPKH",
        UnlockScript::P2sh { .. } => "unlock P2SH",
        UnlockScript::SameAsPrevious => "unlock SameAsPrevious",
        UnlockScript::PoLW(_) => "unlock PoLW",
        UnlockScript::P2pk => "unlock P2PK",
        UnlockScript::P2hmpk { .. } => "unlock P2HMPK",
    }
}

fn lockup_kind(lockup: &LockupScript) -> &'static str {
    match lockup {
        LockupScript::P2pkh(_) => "lockup P2PKH",
        LockupScript::P2mpkh { .. } => "lockup P2MPKH",
        LockupScript::P2sh(_) => "lockup P2SH",
        LockupScript::P2c(_) => "lockup P2C",
        LockupScript::P2pk { .. } => "lockup P2PK",
        LockupScript::P2hmpk { .. } => "lockup P2HMPK",
    }
}
