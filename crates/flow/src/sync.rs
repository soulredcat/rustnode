//! Block following over P2P v2: download by height ranges within the
//! official rate limit, `NewBlock` announcements, and a bounded pool for
//! blocks whose dependencies have not arrived yet.

use std::collections::HashMap;
use std::sync::PoisonError;
use std::time::{Duration, Instant};

pub use alephium_chain::SharedChainStore;
use alephium_chain::{AddError, Added};
use alephium_model::{Block, BlockHeader, Hash};
use alephium_protocol::message::payload::ChainTip;
use alephium_protocol::message::sync::HeightRange;

/// `maxSyncBlocksPerChain`.
pub const MAX_SYNC_BLOCKS_PER_CHAIN: i32 = 50;
/// `MaxRequestNum`: blocks a peer serves per rate-limit window.
pub const MAX_BLOCKS_PER_WINDOW: usize = 50 * 16;
/// Blocks kept while their dependencies are missing.
pub const MAX_ORPHANS: usize = 4096;

/// What the session should send next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRequest {
    pub id: u32,
    pub ranges: Vec<HeightRange>,
}

pub struct BlockSync {
    chain: SharedChainStore,
    groups: u32,
    window_size: Duration,
    window_start: Instant,
    window_used: usize,
    in_flight: Option<(u32, Instant)>,
    peer_tips: Vec<Option<(i32, Hash, Vec<u8>)>>,
    orphans: HashMap<Hash, Block>,
    /// Announced hashes already fetched or held, bounded.
    seen: std::collections::VecDeque<Hash>,
    pub weight_checks: u64,
    pub weight_mismatches: u64,
    pub added: u64,
}

/// Official `getRateLimiterWindowSize` on mainnet.
pub const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(30);

impl BlockSync {
    #[must_use]
    pub fn new(chain: SharedChainStore, groups: u32) -> Self {
        let chains = (groups * groups) as usize;
        Self {
            chain,
            groups,
            window_size: RATE_LIMIT_WINDOW,
            window_start: Instant::now(),
            window_used: 0,
            in_flight: None,
            peer_tips: vec![None; chains],
            orphans: HashMap::new(),
            seen: std::collections::VecDeque::new(),
            weight_checks: 0,
            weight_mismatches: 0,
            added: 0,
        }
    }

    /// Records the peer's tips and, for tips we already hold, checks that our
    /// computed weight equals the peer's.
    pub fn on_chain_state(&mut self, tips: &[ChainTip]) {
        let mut store = self.chain.write().unwrap_or_else(PoisonError::into_inner);
        for tip in tips {
            let hash = Hash(tip.hash.bytes);
            let (from, to) = BlockHeader::chain_index(&hash, self.groups);
            let index = (from * self.groups + to) as usize;
            if let Some(slot) = self.peer_tips.get_mut(index) {
                *slot = Some((tip.height, hash, tip.weight.clone()));
            }
            let Some(store) = store.as_mut() else {
                continue;
            };
            if store.get(&hash).is_some() && !store.has_weight(&hash) {
                // A checkpoint start: weights come from the peer's tips.
                if let Some(weight) = weight_from_bytes(&tip.weight) {
                    let derived = store.seed_weight(&hash, weight);
                    println!("seeded weights from {}: {derived} known", hash.to_hex());
                }
                continue;
            }
            if let Some(weight) = store.get(&hash).and_then(|entry| entry.weight) {
                self.weight_checks += 1;
                if big_integer_bytes(weight) != tip.weight {
                    self.weight_mismatches += 1;
                    eprintln!(
                        "weight mismatch for {}: ours {weight}, peer {:?}",
                        hash.to_hex(),
                        tip.weight
                    );
                }
            }
        }
        if let Some(store) = store.as_mut()
            && let Err(error) = store.flush()
        {
            eprintln!("{error}");
        }
    }

    /// The next height-range request, if the peer is ahead, nothing is in
    /// flight, and the rate-limit window allows it.
    pub fn next_request(&mut self, now: Instant, next_id: &mut u32) -> Option<BlockRequest> {
        if let Some((_, sent)) = self.in_flight {
            if now.duration_since(sent) < self.window_size * 2 {
                return None;
            }
            self.in_flight = None;
        }
        if now.duration_since(self.window_start) >= self.window_size {
            self.window_start = now;
            self.window_used = 0;
        }
        let store = self.chain.read().unwrap_or_else(PoisonError::into_inner);
        let mut ranges = Vec::new();
        let mut size = 0usize;
        for (index, tip) in self.peer_tips.iter().enumerate() {
            let Some((peer_height, _, _)) = tip else {
                continue;
            };
            let (from, to) = (index as u32 / self.groups, index as u32 % self.groups);
            let ours = store.as_ref().map_or(-1, |store| {
                i32::try_from(store.max_height(from, to)).unwrap_or(i32::MAX)
            });
            if *peer_height <= ours {
                continue;
            }
            let from_height = ours + 1;
            let to_height = (*peer_height).min(ours + MAX_SYNC_BLOCKS_PER_CHAIN);
            let count = usize::try_from(to_height - from_height + 1).unwrap_or(0);
            if self.window_used + size + count > MAX_BLOCKS_PER_WINDOW {
                break;
            }
            size += count;
            ranges.push(HeightRange {
                from_group: from as i32,
                to_group: to as i32,
                from_height,
                to_height,
                step: 1,
            });
        }
        drop(store);
        if ranges.is_empty() {
            return None;
        }
        let id = *next_id;
        *next_id = next_id.wrapping_add(1).max(1);
        self.window_used += size;
        self.in_flight = Some((id, now));
        Some(BlockRequest { id, ranges })
    }

    /// Handles a `BlocksAndUnclesByHeightsResponse`; the first response on an
    /// empty store must hold the genesis blocks.
    pub fn on_blocks(&mut self, id: u32, blocks: Vec<Vec<Block>>) -> Vec<Added> {
        if self.in_flight.is_some_and(|(expected, _)| expected == id) {
            self.in_flight = None;
        }
        let mut all: Vec<Block> = blocks.into_iter().flatten().collect();
        {
            let mut store = self.chain.write().unwrap_or_else(PoisonError::into_inner);
            if store.is_none() {
                let (genesis, rest): (Vec<_>, Vec<_>) = all
                    .into_iter()
                    .partition(|block| block.header.timestamp == alephium_chain::GENESIS_TIMESTAMP);
                match self.chain.start(self.groups, genesis) {
                    Ok(new_store) => *store = Some(new_store),
                    Err(error) => {
                        eprintln!("cannot start from the received genesis blocks: {error}");
                        return Vec::new();
                    }
                }
                all = rest;
            }
        }
        self.add_all(all)
    }

    /// Announced blocks (`NewBlock`, or `BlocksResponse` to a hash
    /// announcement); ignored until the store has started.
    pub fn on_new_blocks(&mut self, blocks: Vec<Block>) -> Vec<Added> {
        if self
            .chain
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none()
        {
            return Vec::new();
        }
        self.add_all(blocks)
    }

    /// Whether an announced hash should be downloaded: it must carry at least
    /// mainnet's minimum proof of work (`network::MIN_HASH_ZERO_BITS`) and be
    /// new. The work floor bounds how many fetches a peer can trigger with
    /// hashes it did not actually mine.
    pub fn should_fetch(&mut self, hash: &Hash) -> bool {
        if !BlockHeader::check_min_work(hash) {
            return false;
        }
        let known = self
            .chain
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_none_or(|store| store.contains(hash));
        if known || self.seen.contains(hash) {
            return false;
        }
        if self.seen.len() >= MAX_ORPHANS {
            self.seen.pop_front();
        }
        self.seen.push_back(*hash);
        true
    }

    /// Adds blocks, parking those with missing dependencies and retrying the
    /// parked ones after every successful addition.
    fn add_all(&mut self, blocks: Vec<Block>) -> Vec<Added> {
        let mut added = Vec::new();
        let mut store_guard = self.chain.write().unwrap_or_else(PoisonError::into_inner);
        let Some(store) = store_guard.as_mut() else {
            return added;
        };
        let mut queue = blocks;
        loop {
            let mut progress = false;
            for block in queue.drain(..) {
                let hash = block.hash();
                match store.add(block.clone()) {
                    Ok(result) => {
                        progress = true;
                        self.orphans.remove(&hash);
                        added.push(result);
                    }
                    Err(AddError::MissingDeps(_)) => {
                        if self.orphans.len() < MAX_ORPHANS {
                            self.orphans.insert(hash, block);
                        }
                    }
                    Err(AddError::AlreadyKnown) => {
                        self.orphans.remove(&hash);
                    }
                    Err(error) => {
                        self.orphans.remove(&hash);
                        eprintln!("rejected block {}: {error:?}", hash.to_hex());
                    }
                }
            }
            if !progress || self.orphans.is_empty() {
                break;
            }
            queue = self.orphans.values().cloned().collect();
        }
        // One disk commit for the whole response, before readers see it.
        if let Err(error) = store.flush() {
            eprintln!("{error}");
        }
        self.added += added.len() as u64;
        added
    }

    /// Our heaviest tip per chain, for `ChainState`; `None` before the store
    /// has started.
    #[must_use]
    pub fn chain_state(&self) -> Option<Vec<(Hash, i32, Vec<u8>)>> {
        let store = self.chain.read().unwrap_or_else(PoisonError::into_inner);
        let store = store.as_ref()?;
        let mut tips = Vec::with_capacity((self.groups * self.groups) as usize);
        for from in 0..self.groups {
            for to in 0..self.groups {
                let (hash, height, weight) = store.chain_tip(from, to);
                tips.push((
                    hash,
                    i32::try_from(height).unwrap_or(i32::MAX),
                    big_integer_bytes(weight),
                ));
            }
        }
        Some(tips)
    }

    #[must_use]
    pub fn orphan_count(&self) -> usize {
        self.orphans.len()
    }
}

/// A non-negative Java `BigInteger` that fits 256 bits.
#[must_use]
pub fn weight_from_bytes(bytes: &[u8]) -> Option<alephium_model::U256> {
    if bytes.first().is_some_and(|byte| byte & 0x80 != 0) {
        return None;
    }
    let start = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len());
    let significant = &bytes[start..];
    if significant.len() > 32 {
        return None;
    }
    let mut padded = [0u8; 32];
    padded[32 - significant.len()..].copy_from_slice(significant);
    Some(alephium_model::U256::from_be_bytes(padded))
}

/// `Weight` serde: Java `BigInteger.toByteArray`, minimal two's complement.
#[must_use]
pub fn big_integer_bytes(value: alephium_model::U256) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let start = bytes.iter().position(|byte| *byte != 0).unwrap_or(31);
    let mut out = Vec::with_capacity(33);
    if bytes[start] & 0x80 != 0 {
        out.push(0);
    }
    out.extend_from_slice(&bytes[start..]);
    out
}

#[cfg(test)]
mod tests {
    use alephium_model::U256;

    use super::{big_integer_bytes, weight_from_bytes};

    #[test]
    fn weights_encode_like_java_big_integers() {
        assert_eq!(big_integer_bytes(U256::ZERO), vec![0]);
        assert_eq!(big_integer_bytes(U256::from(127u8)), vec![127]);
        assert_eq!(big_integer_bytes(U256::from(128u8)), vec![0, 128]);
        assert_eq!(big_integer_bytes(U256::from(0x0102u16)), vec![1, 2]);
        for value in [U256::ZERO, U256::from(128u8), U256::MAX] {
            assert_eq!(weight_from_bytes(&big_integer_bytes(value)), Some(value));
        }
        assert_eq!(weight_from_bytes(&[0xff]), None);
    }
}
