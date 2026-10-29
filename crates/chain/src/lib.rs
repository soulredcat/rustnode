#![forbid(unsafe_code)]
//! Block following: per-chain storage of recent blocks, canonical chains,
//! tips, and weights, as the official `BlockHashChain`, `BlockHeaderChain`,
//! `BlockFlow.calWeight`, and `FlowTipsUtil` compute them.
//!
//! Only a window of recent heights is kept per chain; older blocks are pruned
//! as new ones arrive. The whole window is on disk (`alephium-store`) and
//! survives restarts; only the newest [`HOT_HEIGHTS`] heights of each chain
//! stay in memory, and anything older is read from disk.

mod best;
mod difficulty;

pub use best::ViewChange;

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, LockResult, RwLock, RwLockReadGuard, RwLockWriteGuard};

use alephium_model::{Block, BlockHeader, Hash, TimeStamp, U256, deserialize, serialize};
pub use alephium_store::Disk;
use alephium_store::{Meta, Record, WindowStore};

/// The store shared by the P2P session (writer) and the API (readers);
/// `None` until the genesis blocks have arrived. It also carries the window
/// and the disk of a store started from genesis over P2P.
#[derive(Clone)]
pub struct SharedChainStore {
    store: Arc<RwLock<Option<ChainStore>>>,
    window: u32,
    disk: Disk,
}

impl SharedChainStore {
    #[must_use]
    pub fn new(window: u32, disk: Disk) -> Self {
        Self {
            store: Arc::default(),
            window,
            disk,
        }
    }

    pub fn read(&self) -> LockResult<RwLockReadGuard<'_, Option<ChainStore>>> {
        self.store.read()
    }

    pub fn write(&self) -> LockResult<RwLockWriteGuard<'_, Option<ChainStore>>> {
        self.store.write()
    }

    /// A new store from the genesis blocks, with this handle's window and disk.
    pub fn start(&self, groups: u32, genesis: Vec<Block>) -> Result<ChainStore, String> {
        ChainStore::new(groups, self.window, genesis, self.disk.clone())
    }
}

/// `ALPH.GenesisTimestamp`.
pub const GENESIS_TIMESTAMP: TimeStamp = TimeStamp(1_231_006_505_000);
/// `maxForkDepth`.
pub const MAX_FORK_DEPTH: u64 = 200;
/// `ConsensusSettings.tipsPruneDuration`: the longest block target time of
/// Rhone and Danube (16 s) times `maxForkDepth`.
pub const TIPS_PRUNE_MILLIS: u64 = 16_000 * MAX_FORK_DEPTH;
/// Default number of heights kept per chain: 1.6 million blocks over 16
/// chains, about 9 days at the 8 s Danube block time.
pub const DEFAULT_WINDOW: u32 = 100_000;
/// Heights per chain held in memory. New blocks
/// reference tips (at most `TIPS_PRUNE_MILLIS` old, 400 heights at 8 s) and
/// blocks within the fork depth, so lookups below this are rare.
pub const HOT_HEIGHTS: u32 = 1024;
/// Pending disk writes after which a checkpoint import commits.
const IMPORT_FLUSH_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddError {
    /// Hash above target or wrong chain for the hash.
    InvalidPow,
    WrongDepsLength,
    AlreadyKnown,
    /// Dependencies that must be added first.
    MissingDeps(Vec<Hash>),
    /// A dependency is older than the kept window, or the deps are inconsistent.
    Unresolvable(String),
    /// A header rule the official `HeaderValidation` enforces is broken.
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub block: Arc<Block>,
    pub from: u32,
    pub to: u32,
    pub height: u32,
    /// `None` only for blocks whose weight could not be derived (a checkpoint
    /// start without a known weight).
    pub weight: Option<U256>,
}

/// Where a transaction is confirmed on the canonical chain of its chain index
/// (`BlockChain.getTxStatusUnsafe`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxConfirmed {
    pub block: Hash,
    pub index: u32,
    /// `maxHeightByWeight - height + 1`.
    pub chain_confirmations: u32,
}

/// What `add` did, for notifications and logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    pub hash: Hash,
    pub from: u32,
    pub to: u32,
    pub height: u32,
    pub canonical: bool,
}

pub struct ChainStore {
    groups: u32,
    window: u32,
    /// Genesis hash of each chain, `[from * groups + to]`.
    genesis: Vec<Hash>,
    /// Blocks in memory: genesis and the heights in `heights`.
    entries: HashMap<Hash, Entry>,
    /// Hashes per height, canonical first: genesis and the hot heights.
    heights: Vec<BTreeMap<u32, Vec<Hash>>>,
    /// Tips and their timestamps.
    tips: Vec<HashMap<Hash, TimeStamp>>,
    /// The whole window.
    disk: WindowStore,
    /// False on disk while a checkpoint import is unfinished.
    complete: bool,
}

/// `Bytes.byteStringOrdering`: lexicographic over signed bytes.
#[must_use]
pub fn compare_hashes(a: &Hash, b: &Hash) -> Ordering {
    a.0.iter()
        .map(|byte| *byte as i8)
        .cmp(b.0.iter().map(|byte| *byte as i8))
}

/// `BlockHashPool.compareHeight`.
#[must_use]
pub fn compare_height(hash0: &Hash, height0: u32, hash1: &Hash, height1: u32) -> Ordering {
    height0
        .cmp(&height1)
        .then_with(|| compare_hashes(hash0, hash1))
}

/// `Weight.from(target)`: `2^256 / target`, saturating at `U256::MAX`.
#[must_use]
pub fn header_weight(header: &BlockHeader) -> U256 {
    match header.target.value() {
        None => U256::ZERO,
        Some(target) if target == U256::ZERO => U256::MAX,
        Some(target) => {
            let base = U256::MAX / target;
            if target.is_power_of_two() {
                base.saturating_add(U256::ONE)
            } else {
                base
            }
        }
    }
}

fn record_of(entry: &Entry) -> Record {
    Record {
        height: entry.height,
        weight: entry.weight.map(U256::to_be_bytes),
        block: serialize(entry.block.as_ref()),
    }
}

fn raw_hashes(hashes: &[Hash]) -> Vec<[u8; 32]> {
    hashes.iter().map(|hash| hash.0).collect()
}

impl ChainStore {
    /// `genesis` holds the genesis block of every chain, in any order;
    /// whatever the disk held is replaced by this new store.
    pub fn new(groups: u32, window: u32, genesis: Vec<Block>, disk: Disk) -> Result<Self, String> {
        let chains = (groups * groups) as usize;
        let mut disk = WindowStore::new(disk);
        disk.clear().map_err(|error| error.to_string())?;
        let mut store = Self {
            groups,
            window: window.max(1),
            genesis: vec![Hash::ZERO; chains],
            entries: HashMap::new(),
            heights: vec![BTreeMap::new(); chains],
            tips: vec![HashMap::new(); chains],
            disk,
            complete: true,
        };
        for block in genesis {
            let hash = block.hash();
            let (from, to) = BlockHeader::chain_index(&hash, groups);
            let chain = store.chain(from, to);
            if block.header.timestamp != GENESIS_TIMESTAMP {
                return Err(format!("{} is not a genesis block", hash.to_hex()));
            }
            store.genesis[chain] = hash;
            store.heights[chain].insert(0, vec![hash]);
            store.tips[chain].insert(hash, GENESIS_TIMESTAMP);
            store.entries.insert(
                hash,
                Entry {
                    block: Arc::new(block),
                    from,
                    to,
                    height: 0,
                    weight: Some(U256::ZERO),
                },
            );
        }
        if store.genesis.contains(&Hash::ZERO) {
            return Err("missing genesis blocks".into());
        }
        for chain in 0..chains {
            let hash = store.genesis[chain];
            store.persist_entry(&hash);
            store.index_txs(&hash)?;
            store.set_hashes(chain, 0, vec![hash]);
        }
        store.write_meta();
        store.flush_durable()?;
        Ok(store)
    }

    /// Reopens a store kept on disk; `None` when the disk holds no complete
    /// store. Heights below the (possibly smaller) window are pruned.
    pub fn open(groups: u32, window: u32, disk: Disk) -> Result<Option<Self>, String> {
        let window_store = WindowStore::new(disk);
        let Some(meta) = window_store.meta().map_err(|error| error.to_string())? else {
            return Ok(None);
        };
        if !meta.complete {
            return Ok(None);
        }
        let chains = (groups * groups) as usize;
        if meta.groups != groups || meta.genesis.len() != chains {
            return Err(format!(
                "the stored chain has {} groups, expected {groups}",
                meta.groups
            ));
        }
        let mut store = Self {
            groups,
            window: window.max(1),
            genesis: meta.genesis.into_iter().map(Hash).collect(),
            entries: HashMap::new(),
            heights: vec![BTreeMap::new(); chains],
            tips: vec![HashMap::new(); chains],
            disk: window_store,
            complete: true,
        };
        for chain in 0..chains {
            let hash = store.genesis[chain];
            let entry = store
                .get(&hash)
                .ok_or_else(|| format!("stored genesis block {} is missing", hash.to_hex()))?;
            store.heights[chain].insert(0, vec![hash]);
            store.entries.insert(hash, entry);
        }
        store.load_hot()?;
        store.compute_tips();
        for chain in 0..chains {
            store.prune(chain);
        }
        store.flush()?;
        Ok(Some(store))
    }

    const fn chain(&self, from: u32, to: u32) -> usize {
        (from * self.groups + to) as usize
    }

    /// Heights per chain held in memory.
    fn hot_heights(&self) -> u32 {
        HOT_HEIGHTS.min(self.window)
    }

    /// Lowest height of a chain held in memory, genesis aside.
    fn hot_from(&self, chain: usize) -> u32 {
        let max = self.heights[chain].keys().next_back().copied().unwrap_or(0);
        max.saturating_sub(self.hot_heights())
    }

    /// A block of the window, from memory or disk.
    #[must_use]
    pub fn get(&self, hash: &Hash) -> Option<Entry> {
        if let Some(entry) = self.entries.get(hash) {
            return Some(entry.clone());
        }
        let record = match self.disk.record(&hash.0) {
            Ok(record) => record?,
            Err(error) => {
                eprintln!("{error}");
                return None;
            }
        };
        self.entry_from(hash, record)
    }

    fn entry_from(&self, hash: &Hash, record: Record) -> Option<Entry> {
        let block: Block = match deserialize(&record.block) {
            Ok(block) => block,
            Err(error) => {
                eprintln!(
                    "chain store: stored block {} does not decode: {error}",
                    hash.to_hex()
                );
                return None;
            }
        };
        let (from, to) = BlockHeader::chain_index(hash, self.groups);
        Some(Entry {
            block: Arc::new(block),
            from,
            to,
            height: record.height,
            weight: record.weight.map(U256::from_be_bytes),
        })
    }

    /// A block the caller needs; an unknown one is a missing dependency, so a
    /// block whose validation reaches it waits for it instead of failing. A
    /// checkpoint can reference blocks of other chains newer than their
    /// imported tops, which peers then bring.
    fn entry(&self, hash: &Hash) -> Result<Entry, AddError> {
        self.get(hash)
            .ok_or_else(|| AddError::MissingDeps(vec![*hash]))
    }

    #[must_use]
    pub fn contains(&self, hash: &Hash) -> bool {
        self.entries.contains_key(hash)
            || self.disk.contains(&hash.0).unwrap_or_else(|error| {
                eprintln!("{error}");
                false
            })
    }

    fn chain_hashes(&self, chain: usize, height: u32) -> Vec<Hash> {
        if let Some(hashes) = self.heights[chain].get(&height) {
            return hashes.clone();
        }
        match self.disk.hashes_at(chain as u32, height) {
            Ok(hashes) => hashes.into_iter().map(Hash).collect(),
            Err(error) => {
                eprintln!("{error}");
                Vec::new()
            }
        }
    }

    /// Sets the hashes at a height, canonical first: on disk, and in memory
    /// when the height is held there.
    fn set_hashes(&mut self, chain: usize, height: u32, hashes: Vec<Hash>) {
        self.disk
            .put_hashes(chain as u32, height, &raw_hashes(&hashes));
        if self.heights[chain].contains_key(&height) || height >= self.hot_from(chain) {
            self.heights[chain].insert(height, hashes);
        }
    }

    /// Writes a block held in memory to disk.
    fn persist_entry(&mut self, hash: &Hash) {
        if let Some(entry) = self.entries.get(hash) {
            self.disk.put_record(hash.0, &record_of(entry));
        }
    }

    /// Records where each transaction of a block held in memory sits.
    fn index_txs(&mut self, hash: &Hash) -> Result<(), String> {
        let Some(entry) = self.entries.get(hash) else {
            return Ok(());
        };
        for (index, tx) in (0..).zip(&entry.block.transactions) {
            self.disk
                .add_tx_location(tx.id().0, hash.0, index)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn write_meta(&mut self) {
        let meta = Meta {
            groups: self.groups,
            genesis: raw_hashes(&self.genesis),
            complete: self.complete,
        };
        self.disk.put_meta(&meta);
    }

    /// Commits the disk writes of the last changes.
    pub fn flush(&mut self) -> Result<(), String> {
        self.disk.flush().map_err(|error| error.to_string())
    }

    fn flush_durable(&mut self) -> Result<(), String> {
        self.disk.flush_durable().map_err(|error| error.to_string())
    }

    /// Reads the newest heights of every chain from disk into memory.
    fn load_hot(&mut self) -> Result<(), String> {
        let hot = self.hot_heights();
        let disk = &self.disk;
        let mut records = Vec::new();
        for chain in 0..self.heights.len() {
            let Some(max) = disk
                .highest_height(chain as u32)
                .map_err(|error| error.to_string())?
            else {
                continue;
            };
            let rows = disk
                .heights(chain as u32, max.saturating_sub(hot).max(1)..=max)
                .map_err(|error| error.to_string())?;
            for (height, hashes) in rows {
                let hashes: Vec<Hash> = hashes.into_iter().map(Hash).collect();
                for hash in &hashes {
                    let record = disk
                        .record(&hash.0)
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| {
                            format!(
                                "chain store: block {} listed at height {height} is missing",
                                hash.to_hex()
                            )
                        })?;
                    records.push((*hash, record));
                }
                self.heights[chain].insert(height, hashes);
            }
        }
        for (hash, record) in records {
            let entry = self
                .entry_from(&hash, record)
                .ok_or_else(|| format!("chain store: block {} is unreadable", hash.to_hex()))?;
            self.entries.insert(hash, entry);
        }
        Ok(())
    }

    #[must_use]
    pub fn hashes_at(&self, from: u32, to: u32, height: u32) -> Vec<Hash> {
        self.chain_hashes(self.chain(from, to), height)
    }

    /// Highest height held for the chain.
    #[must_use]
    pub fn max_height(&self, from: u32, to: u32) -> u32 {
        self.heights[self.chain(from, to)]
            .keys()
            .next_back()
            .copied()
            .unwrap_or(0)
    }

    /// `BlockChain.maxHeightByWeight`.
    #[must_use]
    pub fn max_height_by_weight(&self, from: u32, to: u32) -> u32 {
        self.max_weight_tip(from, to)
            .map_or(0, |(_, height)| height)
    }

    /// `BlockChain.getTxStatusUnsafe`: the first block holding the
    /// transaction that is canonical on the `from -> to` chain, with its
    /// confirmations; `None` when no canonical block of the window holds it.
    #[must_use]
    pub fn tx_confirmed(&self, tx: &Hash, from: u32, to: u32) -> Option<TxConfirmed> {
        let locations = match self.disk.tx_locations(&tx.0) {
            Ok(locations) => locations,
            Err(error) => {
                eprintln!("{error}");
                return None;
            }
        };
        locations.into_iter().find_map(|(block, index)| {
            let block = Hash(block);
            if BlockHeader::chain_index(&block, self.groups) != (from, to) {
                return None;
            }
            let height = self.get(&block)?.height;
            (self.hashes_at(from, to, height).first() == Some(&block)).then(|| TxConfirmed {
                block,
                index,
                chain_confirmations: (self.max_height_by_weight(from, to) + 1)
                    .saturating_sub(height),
            })
        })
    }

    /// `BlockHashChain.maxWeightTipUnsafe`: the tip of greatest weight, ties
    /// broken by hash; tips of unknown weight count as zero.
    #[must_use]
    pub fn max_weight_tip(&self, from: u32, to: u32) -> Option<(Hash, u32)> {
        let genesis = self.genesis[self.chain(from, to)];
        let mut best: (Hash, u32, U256) = (genesis, 0, U256::ZERO);
        for tip in self.tips[self.chain(from, to)].keys() {
            let Some(entry) = self.entries.get(tip) else {
                continue;
            };
            let weight = entry.weight.unwrap_or(U256::ZERO);
            if weight > best.2 {
                best = (*tip, entry.height, weight);
            }
        }
        Some((best.0, best.1))
    }

    /// Whether every tip of `from -> to` has a known weight: until then its
    /// heaviest tip, and so confirmations, are not known.
    #[must_use]
    pub fn weights_known(&self, from: u32, to: u32) -> bool {
        self.tips[self.chain(from, to)].keys().all(|tip| {
            self.entries
                .get(tip)
                .is_some_and(|entry| entry.weight.is_some())
        })
    }

    /// The chains with a tip of unknown weight, for the following log.
    #[must_use]
    pub fn unweighted_chains(&self) -> usize {
        (0..self.groups)
            .flat_map(|from| (0..self.groups).map(move |to| (from, to)))
            .filter(|(from, to)| !self.weights_known(*from, *to))
            .count()
    }

    /// The heaviest tip as `(hash, height, weight)`, for `ChainState`.
    #[must_use]
    pub fn chain_tip(&self, from: u32, to: u32) -> (Hash, u32, U256) {
        let (hash, height) = self.max_weight_tip(from, to).unwrap_or((Hash::ZERO, 0));
        let weight = self
            .entries
            .get(&hash)
            .and_then(|entry| entry.weight)
            .unwrap_or(U256::ZERO);
        (hash, height, weight)
    }

    fn height(&self, hash: &Hash) -> Result<u32, AddError> {
        Ok(self.entry(hash)?.height)
    }

    /// `BlockHeader.getGroupTip`.
    fn group_tip(&self, hash: &Hash, group: u32) -> Result<Hash, AddError> {
        let entry = self.entry(hash)?;
        if entry.block.header.timestamp == GENESIS_TIMESTAMP {
            return Ok(self.genesis[self.chain(group, group)]);
        }
        let deps = &entry.block.header.deps;
        Ok(match group.cmp(&entry.from) {
            Ordering::Less => deps[group as usize],
            Ordering::Greater => deps[group as usize - 1],
            Ordering::Equal => *hash,
        })
    }

    /// `FlowTipsUtil.getOutTips`: this group's chain tips as seen by the block.
    fn out_tips(&self, hash: &Hash) -> Result<Vec<Hash>, AddError> {
        let entry = self.entry(hash)?;
        if entry.block.header.timestamp == GENESIS_TIMESTAMP {
            let from = entry.from;
            return Ok((0..self.groups)
                .map(|to| self.genesis[self.chain(from, to)])
                .collect());
        }
        let mut tips = entry.block.header.out_deps().to_vec();
        tips[entry.to as usize] = *hash;
        Ok(tips)
    }

    /// `BlockHashChain.getBlockHashesBetween`: blocks after `old` up to `new`.
    fn hashes_between(&self, new: &Hash, old: &Hash) -> Result<Vec<Hash>, AddError> {
        let old_height = self.height(old)?;
        let mut current = *new;
        let mut height = self.height(new)?;
        let mut result = Vec::new();
        while height > old_height {
            result.push(current);
            let entry = self.entry(&current)?;
            current = entry.block.header.deps[(self.groups - 1 + entry.to) as usize];
            height -= 1;
        }
        if current == *old && height == old_height {
            Ok(result)
        } else {
            Err(AddError::Unresolvable(format!(
                "cannot calculate the hashes between {} and {}",
                new.to_hex(),
                old.to_hex()
            )))
        }
    }

    /// `BlockFlow.calWeightUnsafe(header)`.
    fn calc_weight(&self, header: &BlockHeader, from: u32) -> Result<Option<U256>, AddError> {
        let intra_dep = header.deps[(self.groups - 1 + from) as usize];
        let Some(intra_weight) = self.get(&intra_dep).and_then(|entry| entry.weight) else {
            return Ok(None);
        };
        Ok(Some(
            intra_weight.saturating_add(self.weight_increment(header, from)?),
        ))
    }

    /// The weight a block adds on top of its intra-group dependency: its diff
    /// blocks' header weights plus its own (`calWeightUnsafe` minus the
    /// intra-dep weight).
    fn weight_increment(&self, header: &BlockHeader, from: u32) -> Result<U256, AddError> {
        let groups = self.groups as usize;
        let in_deps = &header.deps[..groups - 1];
        let out_deps = &header.deps[groups - 1..];
        let intra_dep = out_deps[from as usize];

        // getFlowTipsUnsafe(intraDep, targetGroup)
        let intra_in_tips: Vec<Hash> = (0..self.groups)
            .filter(|g| *g != from)
            .map(|g| self.group_tip(&intra_dep, g))
            .collect::<Result<_, _>>()?;
        let intra_target_tip = self.group_tip(&intra_dep, from)?;
        let intra_out_tips = self.out_tips(&intra_target_tip)?;

        // getFlowTipsDiffUnsafe(current, intra): group diffs, then out diffs.
        let mut diffs = Vec::new();
        for (new_tip, old_tip) in in_deps.iter().zip(&intra_in_tips) {
            let new_out = self.out_tips(new_tip)?;
            let old_out = self.out_tips(old_tip)?;
            for (new, old) in new_out.iter().zip(&old_out) {
                diffs.extend(self.hashes_between(new, old)?);
            }
        }
        for (new, old) in out_deps.iter().zip(&intra_out_tips) {
            diffs.extend(self.hashes_between(new, old)?);
        }

        let mut increment = header_weight(header);
        for diff in &diffs {
            increment = increment.saturating_add(header_weight(&self.entry(diff)?.block.header));
        }
        Ok(increment)
    }

    /// Starts from a checkpoint instead of genesis: `blocks` holds, per
    /// height, the hashes the official node lists (canonical first). Heights
    /// and canonical order are taken as given; weights stay unknown until
    /// [`seed_weight`](Self::seed_weight). The blocks go straight to disk, and
    /// [`finish_checkpoint`](Self::finish_checkpoint) reads the newest of
    /// them back into memory.
    pub fn import_checkpoint(
        &mut self,
        from: u32,
        to: u32,
        height: u32,
        blocks: Vec<Block>,
    ) -> Result<(), String> {
        let chain = self.chain(from, to);
        if self.complete {
            self.complete = false;
            self.write_meta();
        }
        let mut hashes = self.chain_hashes(chain, height);
        let mut new_blocks = Vec::with_capacity(blocks.len());
        for block in blocks {
            let hash = block.hash();
            if !hashes.contains(&hash) {
                hashes.push(hash);
            }
            if !self.contains(&hash) {
                new_blocks.push((hash, block));
            }
        }
        let disk = &mut self.disk;
        for (hash, block) in new_blocks {
            for (index, tx) in (0..).zip(&block.transactions) {
                disk.add_tx_location(tx.id().0, hash.0, index)
                    .map_err(|error| error.to_string())?;
            }
            disk.put_record(
                hash.0,
                &Record {
                    height,
                    weight: None,
                    block: serialize(&block),
                },
            );
        }
        disk.put_hashes(chain as u32, height, &raw_hashes(&hashes));
        if disk.pending_bytes() >= IMPORT_FLUSH_BYTES {
            disk.flush().map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    /// After a checkpoint import: tips are the blocks without a child, within
    /// the tip-prune duration of the newest block of their chain. The newest
    /// heights are read into memory first, and the store is marked complete
    /// once everything is committed.
    pub fn finish_checkpoint(&mut self) -> Result<(), String> {
        self.flush()?;
        self.load_hot()?;
        self.compute_tips();
        for chain in 0..self.heights.len() {
            self.prune(chain);
        }
        self.complete = true;
        self.write_meta();
        self.flush_durable()
    }

    fn compute_tips(&mut self) {
        let mut parents = HashSet::new();
        for entry in self.entries.values() {
            if entry.block.header.timestamp != GENESIS_TIMESTAMP {
                parents.insert(entry.block.header.deps[(self.groups - 1 + entry.to) as usize]);
            }
        }
        for chain in 0..self.tips.len() {
            let newest = self.heights[chain]
                .values()
                .flatten()
                .filter_map(|hash| self.entries.get(hash))
                .map(|entry| entry.block.header.timestamp.0)
                .max()
                .unwrap_or(0);
            let tips: HashMap<Hash, TimeStamp> = self.heights[chain]
                .values()
                .flatten()
                .filter(|hash| !parents.contains(*hash))
                .filter_map(|hash| self.entries.get(hash).map(|entry| (*hash, entry)))
                .filter(|(_, entry)| entry.block.header.timestamp.0 + TIPS_PRUNE_MILLIS >= newest)
                .map(|(hash, entry)| (hash, entry.block.header.timestamp))
                .collect();
            self.tips[chain] = tips;
        }
    }

    /// Sets a known weight (from a peer's `ChainState`) and derives the
    /// others: backwards along intra-group chains (`weight(parent) =
    /// weight(block) - increment(block)`), then forwards with `calWeight`.
    /// Returns how many weights became known.
    pub fn seed_weight(&mut self, hash: &Hash, weight: U256) -> usize {
        let Some(entry) = self.entries.get_mut(hash) else {
            return 0;
        };
        if entry.weight.is_some() {
            return 0;
        }
        entry.weight = Some(weight);
        let mut derived = vec![*hash];

        // Backwards: an intra-group block's intra dependency is its parent.
        let mut current = *hash;
        while let Some(entry) = self.entries.get(&current) {
            if entry.from != entry.to || entry.block.header.timestamp == GENESIS_TIMESTAMP {
                break;
            }
            let (Some(weight), header, from) = (entry.weight, &entry.block.header, entry.from)
            else {
                break;
            };
            let parent = header.deps[(self.groups - 1 + entry.to) as usize];
            let Ok(increment) = self.weight_increment(header, from) else {
                break;
            };
            match self.entries.get_mut(&parent) {
                Some(parent_entry) if parent_entry.weight.is_none() => {
                    parent_entry.weight = Some(weight.saturating_sub(increment));
                    derived.push(parent);
                    current = parent;
                }
                _ => break,
            }
        }

        // Forwards, in timestamp order, until nothing more can be computed.
        let mut unknown: Vec<(TimeStamp, Hash)> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.weight.is_none())
            .map(|(hash, entry)| (entry.block.header.timestamp, *hash))
            .collect();
        unknown.sort_unstable();
        loop {
            let mut progress = false;
            unknown.retain(|(_, hash)| {
                let entry = &self.entries[hash];
                match self.calc_weight(&entry.block.header, entry.from) {
                    Ok(Some(weight)) => {
                        if let Some(entry) = self.entries.get_mut(hash) {
                            entry.weight = Some(weight);
                        }
                        derived.push(*hash);
                        progress = true;
                        false
                    }
                    _ => true,
                }
            });
            if !progress {
                break;
            }
        }
        for hash in &derived {
            self.persist_entry(hash);
        }
        derived.len()
    }

    /// Whether a block's weight is known.
    #[must_use]
    pub fn has_weight(&self, hash: &Hash) -> bool {
        self.get(hash).is_some_and(|entry| entry.weight.is_some())
    }

    /// The official header rules for a block received from a peer
    /// (`HeaderValidation`): `AlreadyKnown` and `MissingDeps` first, so the
    /// caller can drop or park it, then every rule but the state hash.
    pub fn validate(&self, block: &Block) -> Result<(), AddError> {
        let hash = block.hash();
        if self.contains(&hash) {
            return Err(AddError::AlreadyKnown);
        }
        let header = &block.header;
        if header.deps.len() != (2 * self.groups - 1) as usize {
            return Err(AddError::WrongDepsLength);
        }
        let missing: Vec<Hash> = header
            .deps
            .iter()
            .filter(|dep| !self.contains(dep))
            .copied()
            .collect();
        if !missing.is_empty() {
            return Err(AddError::MissingDeps(missing));
        }
        let (from, to) = BlockHeader::chain_index(&hash, self.groups);
        self.check_header(&hash, header, from, to)
    }

    /// Adds a block whose dependencies are all known: PoW, chain index,
    /// height, weight, canonicality, tips, and the kept window. Blocks from
    /// peers pass [`validate`](Self::validate) first. The writes are
    /// committed by the next [`flush`](Self::flush).
    pub fn add(&mut self, block: Block) -> Result<Added, AddError> {
        let hash = block.hash();
        if self.contains(&hash) {
            return Err(AddError::AlreadyKnown);
        }
        let header = &block.header;
        if header.deps.len() != (2 * self.groups - 1) as usize {
            return Err(AddError::WrongDepsLength);
        }
        let (from, to) = BlockHeader::chain_index(&hash, self.groups);
        if !BlockHeader::check_work(&hash, &header.target) {
            return Err(AddError::InvalidPow);
        }
        let missing: Vec<Hash> = header
            .deps
            .iter()
            .filter(|dep| !self.contains(dep))
            .copied()
            .collect();
        if !missing.is_empty() {
            return Err(AddError::MissingDeps(missing));
        }

        let parent = header.deps[(self.groups - 1 + to) as usize];
        let height = self.height(&parent)? + 1;
        // After a checkpoint start the diff blocks of a weight can reach below
        // the kept window; the block is still valid and its weight comes from
        // a peer's ChainState later (seed_weight).
        let weight = self.calc_weight(header, from).unwrap_or(None);
        let chain = self.chain(from, to);

        // checkCanonicality against the current tips (the parent included).
        let canonical = self.tips[chain].keys().all(|tip| {
            let tip_height = self.entries.get(tip).map_or(0, |entry| entry.height);
            compare_height(&hash, height, tip, tip_height) == Ordering::Greater
        });

        let timestamp = header.timestamp;
        // A late block below the heights held in memory goes to disk only.
        let hot = height >= self.hot_from(chain);
        let entry = Entry {
            block: Arc::new(block),
            from,
            to,
            height,
            weight,
        };
        self.disk.put_record(hash.0, &record_of(&entry));
        for (index, tx) in (0..).zip(&entry.block.transactions) {
            if let Err(error) = self.disk.add_tx_location(tx.id().0, hash.0, index) {
                eprintln!("{error}");
            }
        }
        if hot {
            self.entries.insert(hash, entry);
        }
        let mut at_height = self.chain_hashes(chain, height);
        if canonical {
            at_height.insert(0, hash);
        } else {
            at_height.push(hash);
        }
        self.set_hashes(chain, height, at_height);
        if canonical {
            self.reorg_from(chain, parent, height - 1);
        }

        // updateState: the new block replaces its parent as a tip; stale tips go.
        let tips = &mut self.tips[chain];
        if hot {
            tips.insert(hash, timestamp);
        }
        tips.remove(&parent);
        tips.retain(|_, tip_time| tip_time.0 + TIPS_PRUNE_MILLIS >= timestamp.0);

        self.prune(chain);
        Ok(Added {
            hash,
            from,
            to,
            height,
            canonical,
        })
    }

    /// `BlockHeaderChain.reorgFrom`: ancestors become canonical at their heights.
    fn reorg_from(&mut self, chain: usize, mut hash: Hash, mut height: u32) {
        loop {
            let mut hashes = self.chain_hashes(chain, height);
            if hashes.is_empty() || hashes.first() == Some(&hash) {
                return;
            }
            let Some(position) = hashes.iter().position(|h| *h == hash) else {
                return;
            };
            hashes.remove(position);
            hashes.insert(0, hash);
            self.set_hashes(chain, height, hashes);
            let Some(entry) = self.get(&hash) else {
                return;
            };
            if height == 0 {
                return;
            }
            hash = entry.block.header.deps[(self.groups - 1 + entry.to) as usize];
            height -= 1;
        }
    }

    /// Drops heights below `max height - window` from disk, keeping genesis;
    /// memory keeps only the newest `HOT_HEIGHTS` heights.
    fn prune(&mut self, chain: usize) {
        let max = self.heights[chain].keys().next_back().copied().unwrap_or(0);
        let keep_from = max.saturating_sub(self.window);
        let hot_from = self.hot_from(chain);
        while let Some((&height, _)) = self.heights[chain].iter().nth(1) {
            if height >= hot_from {
                break;
            }
            if let Some(hashes) = self.heights[chain].remove(&height) {
                for hash in hashes {
                    self.entries.remove(&hash);
                    self.tips[chain].remove(&hash);
                }
            }
        }
        if keep_from <= 1 {
            return;
        }
        let rows = match self.disk.heights(chain as u32, 1..=keep_from - 1) {
            Ok(rows) => rows,
            Err(error) => {
                eprintln!("{error}");
                return;
            }
        };
        for (height, hashes) in rows {
            for hash in hashes {
                if let Some(entry) = self.get(&Hash(hash)) {
                    for tx in &entry.block.transactions {
                        if let Err(error) = self.disk.remove_tx_location(tx.id().0, hash) {
                            eprintln!("{error}");
                        }
                    }
                }
                self.disk.delete_record(hash);
            }
            self.disk.delete_hashes(chain as u32, height);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::path::PathBuf;

    use alephium_model::{Block, BlockHeader, Hash, Target, TimeStamp, U256};

    use super::{ChainStore, Disk, GENESIS_TIMESTAMP, HOT_HEIGHTS, compare_hashes, header_weight};

    #[test]
    fn hashes_compare_as_signed_bytes() {
        let mut low = Hash::ZERO;
        let mut high = Hash::ZERO;
        low.0[0] = 0x80; // -128
        high.0[0] = 0x7f; // 127
        assert_eq!(compare_hashes(&low, &high), Ordering::Less);
    }

    #[test]
    fn header_weight_is_two_to_the_256_over_target() {
        let header = |bits: [u8; 4]| BlockHeader {
            nonce: [0; 24],
            version: 0,
            deps: vec![],
            dep_state_hash: Hash::ZERO,
            txs_hash: Hash::ZERO,
            timestamp: TimeStamp(0),
            target: Target(bits),
        };
        // 0x20ffffff: 0xffffff << 232, so 2^256 / target = 1 (floor).
        assert_eq!(header_weight(&header([0x20, 0xff, 0xff, 0xff])), U256::ONE);
        // 0x1d010000: 2^224 exactly, a power of two.
        assert_eq!(
            header_weight(&header([0x1d, 0x01, 0x00, 0x00])),
            U256::ONE << 32
        );
        // 0x03000003: 3.
        assert_eq!(
            header_weight(&header([0x03, 0x00, 0x00, 0x03])),
            U256::MAX / 3
        );
    }

    /// One-group blocks: a single chain whose only dependency is the parent,
    /// each block adding a weight of one.
    fn block(parent: Hash, timestamp: u64) -> Block {
        let mut header = BlockHeader {
            nonce: [0; 24],
            version: 0,
            deps: vec![parent],
            dep_state_hash: Hash::ZERO,
            txs_hash: Hash::ZERO,
            timestamp: TimeStamp(timestamp),
            target: Target([0x20, 0xff, 0xff, 0xff]),
        };
        while !BlockHeader::check_work(&header.hash(), &header.target) {
            header.nonce[0] = header.nonce[0].wrapping_add(1);
        }
        Block {
            header,
            transactions: Vec::new(),
        }
    }

    fn time_of(height: u32) -> u64 {
        GENESIS_TIMESTAMP.0 + u64::from(height) * 8_000
    }

    fn temp_path(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("rustnode-chain-{}-{name}.redb", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn window_on_disk_survives_reopening() {
        let path = temp_path("window");
        let window = 1500;
        let top = HOT_HEIGHTS + 2000;
        let genesis = block(Hash::ZERO, GENESIS_TIMESTAMP.0);
        let mut hashes = vec![genesis.hash()];
        let mut store = ChainStore::new(1, window, vec![genesis], Disk::open(&path).expect("open"))
            .expect("genesis");
        for height in 1..=top {
            let next = block(hashes[height as usize - 1], time_of(height));
            hashes.push(next.hash());
            let added = store.add(next).expect("add");
            assert!(added.canonical);
            if height % 300 == 0 {
                store.flush().expect("flush");
            }
        }
        store.flush().expect("flush");

        let bottom = top - window;
        let at = |height: u32| hashes[height as usize];
        assert!(store.get(&at(bottom - 1)).is_none(), "pruned from disk");
        assert!(!store.contains(&at(bottom - 1)));
        let oldest = store.get(&at(bottom)).expect("oldest kept height");
        assert_eq!(oldest.height, bottom);
        assert_eq!(oldest.weight, Some(U256::from(bottom)));
        assert_eq!(store.hashes_at(0, 0, bottom), vec![at(bottom)]);
        // Memory holds genesis and the hot heights only.
        assert_eq!(store.entries.len(), HOT_HEIGHTS as usize + 2);
        let tip = store.chain_tip(0, 0);
        assert_eq!(tip, (at(top), top, U256::from(top)));
        drop(store);

        let mut store = ChainStore::open(1, window, Disk::open(&path).expect("reopen"))
            .expect("open")
            .expect("a complete store");
        assert_eq!(store.chain_tip(0, 0), tip);
        assert_eq!(store.hashes_at(0, 0, bottom + 1), vec![at(bottom + 1)]);
        assert_eq!(store.hashes_at(0, 0, top), vec![at(top)]);
        assert_eq!(store.entries.len(), HOT_HEIGHTS as usize + 2);
        // The oldest heights are read back from disk.
        assert!(!store.entries.contains_key(&at(bottom)));
        assert_eq!(
            store.get(&at(bottom)).map(|entry| entry.height),
            Some(bottom)
        );

        // Following continues on the reopened store, and a smaller window
        // prunes on the next reopening.
        let next = block(at(top), time_of(top + 1));
        let next_hash = next.hash();
        assert_eq!(store.add(next).map(|added| added.height), Ok(top + 1));
        store.flush().expect("flush");
        drop(store);
        let store = ChainStore::open(1, 200, Disk::open(&path).expect("reopen"))
            .expect("open")
            .expect("a complete store");
        assert_eq!(store.chain_tip(0, 0).0, next_hash);
        assert!(store.get(&at(top - 200)).is_none());
        assert!(store.get(&at(top - 199)).is_some());
        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_unfinished_import_does_not_reopen() {
        let path = temp_path("import");
        let genesis = block(Hash::ZERO, GENESIS_TIMESTAMP.0);
        let first = block(genesis.hash(), time_of(1));
        let second = block(first.hash(), time_of(2));
        let mut store = ChainStore::new(
            1,
            100,
            vec![genesis.clone()],
            Disk::open(&path).expect("open"),
        )
        .expect("genesis");
        store
            .import_checkpoint(0, 0, 1, vec![first.clone()])
            .expect("import");
        store.flush().expect("flush");
        drop(store);
        assert!(
            ChainStore::open(1, 100, Disk::open(&path).expect("reopen"))
                .expect("open")
                .is_none()
        );

        let mut store = ChainStore::new(1, 100, vec![genesis], Disk::open(&path).expect("reopen"))
            .expect("genesis");
        store
            .import_checkpoint(0, 0, 1, vec![first])
            .expect("import");
        store
            .import_checkpoint(0, 0, 2, vec![second.clone()])
            .expect("import");
        store.finish_checkpoint().expect("finish");
        drop(store);
        let store = ChainStore::open(1, 100, Disk::open(&path).expect("reopen"))
            .expect("open")
            .expect("a complete store");
        assert_eq!(
            store.tips[0].keys().copied().collect::<Vec<_>>(),
            vec![second.hash()]
        );
        assert_eq!(store.max_height(0, 0), 2);
        assert!(!store.has_weight(&second.hash()));
        drop(store);
        let _ = std::fs::remove_file(&path);
    }
}
