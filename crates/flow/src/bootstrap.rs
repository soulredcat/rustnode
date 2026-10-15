//! One-time start from the operator's synced official node: the genesis
//! blocks and the last `window` heights of every chain are read through its
//! REST API (`/blockflow/hashes`, `/blockflow/raw-blocks`), so rustnode never
//! replays the chain from genesis. Weights are seeded later from the peer's
//! `ChainState` (`ChainStore::seed_weight`).

use std::sync::Mutex;
use std::time::Duration;

use alephium_chain::ChainStore;
use alephium_model::hash::from_hex;
use alephium_model::{Block, deserialize};

pub struct Source {
    agent: ureq::Agent,
    base: String,
    api_key: Option<String>,
}

impl Source {
    #[must_use]
    pub fn new(base: &str, api_key: Option<String>) -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(30))
                .build(),
            base: base.trim_end_matches('/').to_owned(),
            api_key,
        }
    }

    fn get(&self, path: &str) -> Result<serde_json::Value, String> {
        let mut request = self.agent.get(&format!("{}{path}", self.base));
        if let Some(key) = &self.api_key {
            request = request.set("X-API-KEY", key);
        }
        let body = request
            .call()
            .map_err(|error| format!("GET {path}: {error}"))?
            .into_string()
            .map_err(|error| format!("GET {path}: {error}"))?;
        serde_json::from_str(&body).map_err(|error| format!("GET {path}: {error}"))
    }

    /// All blocks at a height, canonical first, as the official node lists them.
    fn blocks_at(&self, from: u32, to: u32, height: u32) -> Result<Vec<Block>, String> {
        let hashes = self.get(&format!(
            "/blockflow/hashes?fromGroup={from}&toGroup={to}&height={height}"
        ))?;
        let hashes = hashes["headers"]
            .as_array()
            .ok_or_else(|| format!("no hashes at {from}->{to} height {height}"))?;
        hashes
            .iter()
            .map(|hash| {
                let hash = hash.as_str().ok_or("hash is not a string")?;
                let raw = self.get(&format!("/blockflow/raw-blocks/{hash}"))?;
                let bytes = raw["value"]
                    .as_str()
                    .and_then(from_hex)
                    .ok_or_else(|| format!("bad raw block {hash}"))?;
                let block: Block =
                    deserialize(&bytes).map_err(|error| format!("decode {hash}: {error}"))?;
                if block.hash().to_hex() == hash {
                    Ok(block)
                } else {
                    Err(format!("raw block {hash} hashes differently"))
                }
            })
            .collect()
    }
}

/// Builds a store holding genesis plus the last `window` heights per chain.
pub fn bootstrap(
    source: &Source,
    groups: u32,
    network_id: u8,
    window: u32,
    threads: usize,
) -> Result<ChainStore, String> {
    let params = source.get("/infos/chain-params")?;
    if params["groups"].as_u64() != Some(u64::from(groups))
        || params["networkId"].as_u64() != Some(u64::from(network_id))
    {
        return Err(format!(
            "the official node runs network {} with {} groups, expected network {network_id} with {groups}",
            params["networkId"], params["groups"]
        ));
    }

    let mut genesis = Vec::new();
    let mut tasks = Vec::new();
    for from in 0..groups {
        for to in 0..groups {
            genesis.extend(source.blocks_at(from, to, 0)?);
            let info = source.get(&format!(
                "/blockflow/chain-info?fromGroup={from}&toGroup={to}"
            ))?;
            let height = info["currentHeight"]
                .as_u64()
                .and_then(|height| u32::try_from(height).ok())
                .ok_or("no current height")?;
            let start = height.saturating_sub(window).max(1);
            tasks.extend((start..=height).map(|h| (from, to, h)));
        }
    }
    let mut store = ChainStore::new(groups, window, genesis)?;

    println!(
        "bootstrap: fetching {} heights from {}",
        tasks.len(),
        source.base
    );
    let queue = Mutex::new(tasks.into_iter());
    let results = Mutex::new(Vec::new());
    let failure = Mutex::new(None::<String>);
    std::thread::scope(|scope| {
        for _ in 0..threads.max(1) {
            scope.spawn(|| {
                loop {
                    let next = queue.lock().ok().and_then(|mut queue| queue.next());
                    let Some((from, to, height)) = next else {
                        return;
                    };
                    match source.blocks_at(from, to, height) {
                        Ok(blocks) => {
                            if let Ok(mut results) = results.lock() {
                                results.push((from, to, height, blocks));
                            }
                        }
                        Err(error) => {
                            if let Ok(mut failure) = failure.lock() {
                                *failure = Some(error);
                            }
                            return;
                        }
                    }
                }
            });
        }
    });
    if let Some(error) = failure.into_inner().ok().flatten() {
        return Err(error);
    }
    let mut results = results.into_inner().unwrap_or_default();
    results.sort_by_key(|(from, to, height, _)| (*from, *to, *height));
    let blocks: usize = results.iter().map(|(_, _, _, blocks)| blocks.len()).sum();
    for (from, to, height, blocks) in results {
        store.import_checkpoint(from, to, height, blocks);
    }
    store.finish_checkpoint();
    println!("bootstrap: imported {blocks} blocks");
    Ok(store)
}
