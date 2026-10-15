#![forbid(unsafe_code)]
//! Block following: per-chain storage of recent blocks, canonical chains,
//! tips, and weights, as the official `BlockHashChain`, `BlockHeaderChain`,
//! `BlockFlow.calWeight`, and `FlowTipsUtil` compute them.
//!
//! Only a window of recent heights is kept per chain; older blocks are pruned.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

use alephium_model::{Block, BlockHeader, Hash, TimeStamp, U256};

/// The store shared by the P2P session (writer) and the API (readers);
/// `None` until the genesis blocks have arrived.
pub type SharedChainStore = Arc<RwLock<Option<ChainStore>>>;

/// `ALPH.GenesisTimestamp`.
pub const GENESIS_TIMESTAMP: TimeStamp = TimeStamp(1_231_006_505_000);
/// `maxForkDepth`.
pub const MAX_FORK_DEPTH: u64 = 200;
/// `ConsensusSettings.tipsPruneDuration`: the longest block target time of
/// Rhone and Danube (16 s) times `maxForkDepth`.
pub const TIPS_PRUNE_MILLIS: u64 = 16_000 * MAX_FORK_DEPTH;
/// Default number of heights kept per chain.
pub const DEFAULT_WINDOW: u32 = 10_000;

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
    entries: HashMap<Hash, Entry>,
    /// Hashes per height, canonical first.
    heights: Vec<BTreeMap<u32, Vec<Hash>>>,
    /// Tips and their timestamps.
    tips: Vec<HashMap<Hash, TimeStamp>>,
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

impl ChainStore {
    /// `genesis` holds the genesis block of every chain, in any order.
    pub fn new(groups: u32, window: u32, genesis: Vec<Block>) -> Result<Self, String> {
        let chains = (groups * groups) as usize;
        let mut store = Self {
            groups,
            window: window.max(1),
            genesis: vec![Hash::ZERO; chains],
            entries: HashMap::new(),
            heights: vec![BTreeMap::new(); chains],
            tips: vec![HashMap::new(); chains],
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
        Ok(store)
    }

    #[must_use]
    pub const fn groups(&self) -> u32 {
        self.groups
    }

    const fn chain(&self, from: u32, to: u32) -> usize {
        (from * self.groups + to) as usize
    }

    #[must_use]
    pub fn get(&self, hash: &Hash) -> Option<&Entry> {
        self.entries.get(hash)
    }

    #[must_use]
    pub fn contains(&self, hash: &Hash) -> bool {
        self.entries.contains_key(hash)
    }

    #[must_use]
    pub fn hashes_at(&self, from: u32, to: u32, height: u32) -> &[Hash] {
        self.heights[self.chain(from, to)]
            .get(&height)
            .map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn is_canonical(&self, hash: &Hash) -> bool {
        self.entries.get(hash).is_some_and(|entry| {
            self.hashes_at(entry.from, entry.to, entry.height).first() == Some(hash)
        })
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

    /// Lowest height still held for the chain.
    #[must_use]
    pub fn min_height(&self, from: u32, to: u32) -> u32 {
        self.heights[self.chain(from, to)]
            .keys()
            .next()
            .copied()
            .unwrap_or(0)
    }

    #[must_use]
    pub fn tips(&self, from: u32, to: u32) -> Vec<Hash> {
        self.tips[self.chain(from, to)].keys().copied().collect()
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

    /// `BlockChain.maxHeightByWeight`.
    #[must_use]
    pub fn max_height_by_weight(&self, from: u32, to: u32) -> u32 {
        self.max_weight_tip(from, to)
            .map_or(0, |(_, height)| height)
    }

    /// The canonical block at a height.
    #[must_use]
    pub fn canonical_at(&self, from: u32, to: u32, height: u32) -> Option<&Entry> {
        self.hashes_at(from, to, height)
            .first()
            .and_then(|hash| self.entries.get(hash))
    }

    fn is_genesis(&self, hash: &Hash) -> bool {
        self.entries
            .get(hash)
            .is_some_and(|entry| entry.block.header.timestamp == GENESIS_TIMESTAMP)
    }

    fn header(&self, hash: &Hash) -> Result<&BlockHeader, AddError> {
        self.entries
            .get(hash)
            .map(|entry| &entry.block.header)
            .ok_or_else(|| AddError::Unresolvable(format!("unknown block {}", hash.to_hex())))
    }

    fn height(&self, hash: &Hash) -> Result<u32, AddError> {
        self.entries
            .get(hash)
            .map(|entry| entry.height)
            .ok_or_else(|| AddError::Unresolvable(format!("unknown block {}", hash.to_hex())))
    }

    /// `BlockHeader.getGroupTip`.
    fn group_tip(&self, hash: &Hash, group: u32) -> Result<Hash, AddError> {
        let entry = self
            .entries
            .get(hash)
            .ok_or_else(|| AddError::Unresolvable(format!("unknown block {}", hash.to_hex())))?;
        if self.is_genesis(hash) {
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
        let entry = self
            .entries
            .get(hash)
            .ok_or_else(|| AddError::Unresolvable(format!("unknown block {}", hash.to_hex())))?;
        if self.is_genesis(hash) {
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
            let header = self.header(&current)?;
            let entry = &self.entries[&current];
            current = header.deps[(self.groups - 1 + entry.to) as usize];
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
        let Some(intra_weight) = self.entries.get(&intra_dep).and_then(|entry| entry.weight) else {
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
            increment = increment.saturating_add(header_weight(self.header(diff)?));
        }
        Ok(increment)
    }

    /// Starts from a checkpoint instead of genesis: `blocks` holds, per
    /// height, the hashes the official node lists (canonical first). Heights
    /// and canonical order are taken as given; weights stay unknown until
    /// [`seed_weight`](Self::seed_weight).
    pub fn import_checkpoint(&mut self, from: u32, to: u32, height: u32, blocks: Vec<Block>) {
        let chain = self.chain(from, to);
        let mut hashes = Vec::with_capacity(blocks.len());
        for block in blocks {
            let hash = block.hash();
            hashes.push(hash);
            self.entries.entry(hash).or_insert_with(|| Entry {
                block: Arc::new(block),
                from,
                to,
                height,
                weight: None,
            });
        }
        let at_height = self.heights[chain].entry(height).or_default();
        for hash in hashes {
            if !at_height.contains(&hash) {
                at_height.push(hash);
            }
        }
    }

    /// After a checkpoint import: tips are the blocks without a child, within
    /// the tip-prune duration of the newest block of their chain.
    pub fn finish_checkpoint(&mut self) {
        let mut parents = std::collections::HashSet::new();
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
        let mut derived = 1;

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
                    derived += 1;
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
                        derived += 1;
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
        derived
    }

    /// Whether a block's weight is known.
    #[must_use]
    pub fn has_weight(&self, hash: &Hash) -> bool {
        self.entries
            .get(hash)
            .is_some_and(|entry| entry.weight.is_some())
    }

    /// Adds a block whose dependencies are all known: PoW, chain index,
    /// height, weight, canonicality, tips, and the kept window.
    pub fn add(&mut self, block: Block) -> Result<Added, AddError> {
        let hash = block.hash();
        if self.entries.contains_key(&hash) {
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
            .filter(|dep| !self.entries.contains_key(dep))
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
        self.entries.insert(
            hash,
            Entry {
                block: Arc::new(block),
                from,
                to,
                height,
                weight,
            },
        );
        let at_height = self.heights[chain].entry(height).or_default();
        if canonical {
            at_height.insert(0, hash);
            self.reorg_from(chain, parent, height - 1);
        } else {
            at_height.push(hash);
        }

        // updateState: the new block replaces its parent as a tip; stale tips go.
        let tips = &mut self.tips[chain];
        tips.insert(hash, timestamp);
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
            let Some(hashes) = self.heights[chain].get_mut(&height) else {
                return;
            };
            if hashes.first() == Some(&hash) {
                return;
            }
            let Some(position) = hashes.iter().position(|h| *h == hash) else {
                return;
            };
            hashes.remove(position);
            hashes.insert(0, hash);
            let Some(entry) = self.entries.get(&hash) else {
                return;
            };
            if height == 0 {
                return;
            }
            hash = entry.block.header.deps[(self.groups - 1 + entry.to) as usize];
            height -= 1;
        }
    }

    /// Drops heights below `max height - window`, keeping genesis.
    fn prune(&mut self, chain: usize) {
        let max = self.heights[chain].keys().next_back().copied().unwrap_or(0);
        let keep_from = max.saturating_sub(self.window);
        while let Some((&height, _)) = self.heights[chain].iter().nth(1) {
            if height >= keep_from {
                break;
            }
            if let Some(hashes) = self.heights[chain].remove(&height) {
                for hash in hashes {
                    self.entries.remove(&hash);
                    self.tips[chain].remove(&hash);
                }
            }
        }
    }

    /// Main-chain blocks of a chain with timestamps in `[from_ts, to_ts]`, up
    /// to the height of the heaviest tip (`BlockChain.getHeightedBlocks`).
    #[must_use]
    pub fn blocks_between(
        &self,
        from: u32,
        to: u32,
        from_ts: TimeStamp,
        to_ts: TimeStamp,
    ) -> Vec<(Arc<Block>, u32)> {
        let top = self.max_height_by_weight(from, to);
        let bottom = self.min_height(from, to);
        let mut result = Vec::new();
        for height in bottom..=top {
            if let Some(entry) = self.canonical_at(from, to, height) {
                let ts = entry.block.header.timestamp;
                if ts >= from_ts && ts <= to_ts {
                    result.push((Arc::clone(&entry.block), height));
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use alephium_model::{BlockHeader, Hash, Target, TimeStamp, U256};

    use super::{compare_hashes, header_weight};

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
}
