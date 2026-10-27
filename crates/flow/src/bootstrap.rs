//! Where following starts. A chain kept on disk is reopened; when the
//! operator's synced official node agrees with it, only the heights missed
//! while rustnode was stopped are fetched. Otherwise genesis and the last
//! `window` heights of every chain are read through the official node's REST
//! API (`/blockflow/hashes`, `/blockflow/raw-blocks`), so rustnode never
//! replays the chain from genesis. Without a usable official node a stored
//! chain is followed as it is. Weights of fetched blocks are seeded later from
//! the peer's `ChainState` (`ChainStore::seed_weight`).

use std::sync::Mutex;
use std::time::Duration;

use alephium_chain::{ChainStore, Disk, MAX_FORK_DEPTH};
use alephium_model::hash::from_hex;
use alephium_model::{Block, Hash, deserialize};

/// Heights fetched in parallel before they are imported together.
const CHUNK_HEIGHTS: usize = 4096;
/// Heights between progress lines.
const LOG_EVERY: usize = 100_000;

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

    fn check_params(&self, groups: u32, network_id: u8) -> Result<(), String> {
        let params = self.get("/infos/chain-params")?;
        if params["groups"].as_u64() == Some(u64::from(groups))
            && params["networkId"].as_u64() == Some(u64::from(network_id))
        {
            Ok(())
        } else {
            Err(format!(
                "the official node runs network {} with {} groups, expected network {network_id} with {groups}",
                params["networkId"], params["groups"]
            ))
        }
    }

    fn current_height(&self, from: u32, to: u32) -> Result<u32, String> {
        let info = self.get(&format!(
            "/blockflow/chain-info?fromGroup={from}&toGroup={to}"
        ))?;
        info["currentHeight"]
            .as_u64()
            .and_then(|height| u32::try_from(height).ok())
            .ok_or_else(|| "no current height".to_owned())
    }

    /// The hashes at a height, canonical first, as the official node lists them.
    fn hashes_at(&self, from: u32, to: u32, height: u32) -> Result<Vec<String>, String> {
        let hashes = self.get(&format!(
            "/blockflow/hashes?fromGroup={from}&toGroup={to}&height={height}"
        ))?;
        hashes["headers"]
            .as_array()
            .ok_or_else(|| format!("no hashes at {from}->{to} height {height}"))?
            .iter()
            .map(|hash| {
                hash.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "hash is not a string".to_owned())
            })
            .collect()
    }

    /// All blocks at a height, canonical first.
    fn blocks_at(&self, from: u32, to: u32, height: u32) -> Result<Vec<Block>, String> {
        self.hashes_at(from, to, height)?
            .iter()
            .map(|hash| {
                let raw = self.get(&format!("/blockflow/raw-blocks/{hash}"))?;
                let bytes = raw["value"]
                    .as_str()
                    .and_then(from_hex)
                    .ok_or_else(|| format!("bad raw block {hash}"))?;
                let block: Block =
                    deserialize(&bytes).map_err(|error| format!("decode {hash}: {error}"))?;
                if block.hash().to_hex() == *hash {
                    Ok(block)
                } else {
                    Err(format!("raw block {hash} hashes differently"))
                }
            })
            .collect()
    }
}

/// Whether the official node holds the stored chain: the same genesis, the
/// same canonical block `maxForkDepth` below the lower of the two tips, and
/// no more than a window of heights ahead (chain 0 -> 0 stands for all).
/// `Ok(Some(reason))` when it does not; `Err` when it cannot be asked.
fn disagreement(
    source: &Source,
    store: &ChainStore,
    window: u32,
) -> Result<Option<String>, String> {
    let official_genesis = source.hashes_at(0, 0, 0)?;
    let ours = store.hashes_at(0, 0, 0);
    if official_genesis.first().map(String::as_str) != ours.first().map(Hash::to_hex).as_deref() {
        return Ok(Some("different genesis".into()));
    }
    let ours_height = store.max_height(0, 0);
    let official_height = source.current_height(0, 0)?;
    if official_height > ours_height.saturating_add(window) {
        return Ok(Some(format!(
            "{} heights behind, more than the window",
            official_height - ours_height
        )));
    }
    let check = ours_height
        .min(official_height)
        .saturating_sub(u32::try_from(MAX_FORK_DEPTH).unwrap_or(u32::MAX));
    let Some(ours) = store.hashes_at(0, 0, check).first().map(Hash::to_hex) else {
        return Ok(None);
    };
    let official = source.hashes_at(0, 0, check)?;
    if official.first() == Some(&ours) {
        Ok(None)
    } else {
        Ok(Some(format!("a different block at height {check}")))
    }
}

/// The chain to follow: the stored one, caught up from the official node
/// when it agrees, or genesis plus the last `window` heights of every chain.
pub fn start(
    source: &Source,
    groups: u32,
    network_id: u8,
    window: u32,
    threads: usize,
    disk: Option<Disk>,
) -> Result<ChainStore, String> {
    let stored = match &disk {
        Some(disk) => ChainStore::open(groups, window, disk.clone()).unwrap_or_else(|error| {
            println!(
                "bootstrap: the stored chain cannot be used ({error}); fetching the window again"
            );
            None
        }),
        None => None,
    };
    if let Some(mut store) = stored {
        let verdict = source
            .check_params(groups, network_id)
            .and_then(|()| disagreement(source, &store, window));
        match verdict {
            Ok(None) => {
                let tasks = missed_heights(source, &store, groups);
                // Whatever was fetched before a failure is gapless and kept;
                // peers bring the rest.
                let caught_up = tasks.and_then(|tasks| import(source, &mut store, tasks, threads));
                store.finish_checkpoint()?;
                match caught_up {
                    Ok(()) => println!("bootstrap: continuing from the stored chain"),
                    Err(error) => println!(
                        "bootstrap: catching up from the official node stopped ({error}); continuing from the stored chain"
                    ),
                }
                return Ok(store);
            }
            Ok(Some(reason)) => println!(
                "bootstrap: the stored chain does not match the official node ({reason}); fetching the window again"
            ),
            Err(error) => {
                println!(
                    "bootstrap: the official node cannot be used ({error}); continuing from the stored chain"
                );
                return Ok(store);
            }
        }
    } else {
        source.check_params(groups, network_id)?;
    }

    let mut genesis = Vec::new();
    let mut tasks = Vec::new();
    for from in 0..groups {
        for to in 0..groups {
            genesis.extend(source.blocks_at(from, to, 0)?);
            let height = source.current_height(from, to)?;
            let first = height.saturating_sub(window).max(1);
            tasks.extend((first..=height).map(|h| (from, to, h)));
        }
    }
    let mut store = ChainStore::new(groups, window, genesis, disk)?;
    import(source, &mut store, tasks, threads)?;
    store.finish_checkpoint()?;
    Ok(store)
}

/// Heights the official node has above the stored chain's.
fn missed_heights(
    source: &Source,
    store: &ChainStore,
    groups: u32,
) -> Result<Vec<(u32, u32, u32)>, String> {
    let mut tasks = Vec::new();
    for from in 0..groups {
        for to in 0..groups {
            let height = source.current_height(from, to)?;
            let first = store.max_height(from, to) + 1;
            tasks.extend((first..=height).map(|h| (from, to, h)));
        }
    }
    Ok(tasks)
}

/// Fetches and imports `(from, to, height)` tasks in chunks, in height
/// order: a chunk is imported only once all of it has arrived, so after a
/// failure every chain still holds a gapless run of heights.
fn import(
    source: &Source,
    store: &mut ChainStore,
    mut tasks: Vec<(u32, u32, u32)>,
    threads: usize,
) -> Result<(), String> {
    tasks.sort_unstable_by_key(|&(from, to, height)| (height, from, to));
    let total = tasks.len();
    if total == 0 {
        return Ok(());
    }
    println!("bootstrap: fetching {total} heights from {}", source.base);
    let mut done = 0;
    let mut blocks = 0;
    let mut next_log = LOG_EVERY;
    for chunk in tasks.chunks(CHUNK_HEIGHTS) {
        for (from, to, height, fetched) in fetch(source, chunk, threads)? {
            blocks += fetched.len();
            store.import_checkpoint(from, to, height, fetched)?;
        }
        done += chunk.len();
        if done >= next_log {
            println!("bootstrap: {done} of {total} heights");
            next_log += LOG_EVERY;
        }
    }
    println!("bootstrap: imported {blocks} blocks");
    Ok(())
}

type Fetched = (u32, u32, u32, Vec<Block>);

fn fetch(
    source: &Source,
    chunk: &[(u32, u32, u32)],
    threads: usize,
) -> Result<Vec<Fetched>, String> {
    let queue = Mutex::new(chunk.iter().copied());
    let results = Mutex::new(Vec::with_capacity(chunk.len()));
    let failure = Mutex::new(None::<String>);
    std::thread::scope(|scope| {
        for _ in 0..threads.max(1) {
            scope.spawn(|| {
                loop {
                    if !failure.lock().is_ok_and(|failure| failure.is_none()) {
                        return;
                    }
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
    let results = results.into_inner().unwrap_or_default();
    if results.len() == chunk.len() {
        Ok(results)
    } else {
        Err(format!(
            "fetched {} of {} heights",
            results.len(),
            chunk.len()
        ))
    }
}
