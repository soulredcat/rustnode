//! World state bootstrap: the world state after a settled block of every
//! intra-group chain is copied from the operator's official node database,
//! and must hash to the `depStateHash` of the block built on it.

use std::path::Path;
use std::sync::PoisonError;
use std::time::Instant;

use alephium_model::Hash;
use alephium_state::{Official, StateStore, copy_state};

use crate::sync::SharedChainStore;

/// Heights below the newest at which a block is taken as settled.
const SETTLED_DEPTH: u32 = 20;

/// Copies the world state of every group from the database at
/// `official_db` into `store`; the database is read as a secondary
/// instance, which keeps its own files in `secondary`.
pub fn bootstrap(
    official_db: &Path,
    store: &StateStore,
    secondary: &Path,
    chain: &SharedChainStore,
    groups: u32,
) -> Result<(), String> {
    let official = Official::open(official_db, secondary)?;
    for group in 0..groups {
        let (block, child, dep_state_hash) = settled_pair(chain, group, groups)?;
        let started = Instant::now();
        let (roots, copied) = copy_state(&official, store, &block)?;
        if roots.state_hash() != dep_state_hash {
            return Err(format!(
                "group {group}: the world state after {} hashes to {}, but {} commits to {}",
                block.to_hex(),
                roots.state_hash().to_hex(),
                child.to_hex(),
                dep_state_hash.to_hex()
            ));
        }
        println!(
            "state: group {group} after block {}: {} nodes ({} MiB, {} contracts) copied in {:.0?}; its hash is the depStateHash of {}",
            block.to_hex(),
            copied.nodes,
            copied.bytes / (1024 * 1024),
            copied.contracts,
            started.elapsed(),
            child.to_hex()
        );
    }
    Ok(())
}

/// A settled block of `group -> group` and the block on top of it: its
/// child on that chain, whose intra-group dependency it is.
fn settled_pair(
    chain: &SharedChainStore,
    group: u32,
    groups: u32,
) -> Result<(Hash, Hash, Hash), String> {
    let guard = chain.read().unwrap_or_else(PoisonError::into_inner);
    let store = guard.as_ref().ok_or("no followed chain")?;
    let height = store
        .max_height(group, group)
        .checked_sub(SETTLED_DEPTH)
        .ok_or("the chain is too short")?;
    for child in store.hashes_at(group, group, height + 1) {
        let Some(entry) = store.get(&child) else {
            continue;
        };
        let parent = entry.block.header.deps[(groups - 1 + group) as usize];
        if store.hashes_at(group, group, height).contains(&parent) {
            return Ok((parent, child, entry.block.header.dep_state_hash));
        }
    }
    Err(format!(
        "group {group}: no block at height {height} with a child"
    ))
}

/// Follows the world state block by block from the bootstrapped roots: each
/// intra-group block whose parent's state is kept gets its own, and every
/// block's `depStateHash` must equal the state of the intra-group block it
/// builds on. A mismatch stops the follower: rustnode must not serve a state
/// the network disagrees with.
pub struct Follower {
    store: std::sync::Arc<StateStore>,
    groups: u32,
    /// Out-blocks with transactions skipped as conflicted, by the
    /// intra-group block that applied them (`ConflictedTxsStorage`).
    conflicts: std::collections::HashMap<Hash, Vec<(Hash, Vec<Hash>)>>,
    /// Blocks whose `depStateHash` was checked, with their height.
    checked: std::collections::HashMap<Hash, u32>,
    /// Per group, the height the next step starts from: a little below the
    /// newest kept state, found by searching the window once.
    resume: Vec<Option<u32>>,
    pub applied: u64,
    pub verified: u64,
}

impl Follower {
    #[must_use]
    pub fn new(store: std::sync::Arc<StateStore>, groups: u32) -> Self {
        Self {
            store,
            groups,
            conflicts: std::collections::HashMap::new(),
            checked: std::collections::HashMap::new(),
            resume: vec![None; groups as usize],
            applied: 0,
            verified: 0,
        }
    }

    /// Applies every block whose parent's state is kept, oldest first, and
    /// checks the `depStateHash` of every block on a kept state.
    pub fn step(&mut self, chain: &alephium_chain::ChainStore) -> Result<(), String> {
        for group in 0..self.groups {
            let top = chain.max_height(group, group);
            let bottom = match self.resume[group as usize] {
                Some(bottom) => bottom,
                None => self.newest_kept(chain, group, top)?,
            };
            let mut newest = bottom;
            for height in bottom..=top {
                for hash in chain.hashes_at(group, group, height) {
                    if self.store.roots(&hash)?.is_some() {
                        continue;
                    }
                    let Some(entry) = chain.get(&hash) else {
                        continue;
                    };
                    let parent = entry.block.header.deps[(self.groups - 1 + group) as usize];
                    let Some(parent_roots) = self.store.roots(&parent)? else {
                        continue;
                    };
                    newest = newest.max(height);
                    let mut world =
                        alephium_state::WorldState::new(&parent_roots, self.store.as_ref());
                    let conflicts = &self.conflicts;
                    let groups = self.groups;
                    let deps = entry.block.header.deps.clone();
                    let applied = crate::execute::apply_block(
                        chain,
                        &mut world,
                        &entry.block,
                        groups,
                        |in_block, tx| in_conflicted(chain, conflicts, &deps, groups, in_block, tx),
                    )
                    .map_err(|error| format!("block {}: {error}", hash.to_hex()))?;
                    for (out_block, txs) in applied.conflicts {
                        self.conflicts
                            .entry(out_block)
                            .or_default()
                            .push((hash, txs));
                    }
                    let roots = world.roots();
                    self.store.put_state(&hash, &roots, world.new_nodes())?;
                    self.applied += 1;
                }
            }
            // Late forks below the newest state are picked up on the way.
            self.resume[group as usize] = Some(newest.saturating_sub(FOLLOW_DEPTH).max(bottom));
        }
        for from in 0..self.groups {
            for to in 0..self.groups {
                let top = chain.max_height(from, to);
                for height in top.saturating_sub(FOLLOW_DEPTH)..=top {
                    for hash in chain.hashes_at(from, to, height) {
                        if self.checked.contains_key(&hash) {
                            continue;
                        }
                        let Some(entry) = chain.get(&hash) else {
                            continue;
                        };
                        let dep = entry.block.header.deps[(self.groups - 1 + from) as usize];
                        let Some(roots) = self.store.roots(&dep)? else {
                            continue;
                        };
                        if roots.state_hash() != entry.block.header.dep_state_hash {
                            return Err(format!(
                                "block {} commits to state {}, but the state after {} is {}",
                                hash.to_hex(),
                                entry.block.header.dep_state_hash.to_hex(),
                                dep.to_hex(),
                                roots.state_hash().to_hex()
                            ));
                        }
                        self.checked.insert(hash, height);
                        self.verified += 1;
                    }
                }
            }
        }
        let lowest = (0..self.groups)
            .flat_map(|from| (0..self.groups).map(move |to| (from, to)))
            .map(|(from, to)| chain.max_height(from, to).saturating_sub(FOLLOW_DEPTH))
            .min()
            .unwrap_or(0);
        self.checked.retain(|_, height| *height >= lowest);
        Ok(())
    }

    /// The newest height of `group -> group` whose block has a kept state,
    /// searched down through the window; `top` when none has.
    fn newest_kept(
        &self,
        chain: &alephium_chain::ChainStore,
        group: u32,
        top: u32,
    ) -> Result<u32, String> {
        for height in (top.saturating_sub(chain.window())..=top).rev() {
            for hash in chain.hashes_at(group, group, height) {
                if self.store.roots(&hash)?.is_some() {
                    return Ok(height);
                }
            }
        }
        Ok(top)
    }
}

/// Heights below the newest that the follower looks at.
const FOLLOW_DEPTH: u32 = 64;

/// `isTxConflicted`: a transaction of `in_block` was skipped where it came
/// from by an intra-group block in the view of the applying block's
/// dependency on that group.
fn in_conflicted(
    chain: &alephium_chain::ChainStore,
    conflicts: &std::collections::HashMap<Hash, Vec<(Hash, Vec<Hash>)>>,
    deps: &[Hash],
    groups: u32,
    in_block: &Hash,
    tx: &Hash,
) -> bool {
    let Some(sources) = conflicts.get(in_block) else {
        return false;
    };
    let (from, to) = alephium_model::BlockHeader::chain_index(in_block, groups);
    // `getCheckpointHash`: the applying block's in-dependency on `from`.
    let index = if from < to { from } else { from - 1 } as usize;
    let checkpoint = deps[index];
    sources.iter().any(|(intra, txs)| {
        txs.contains(tx) && chain.in_block_flow(&checkpoint, intra).unwrap_or(false)
    })
}

/// How often the follower looks for new blocks, and logs.
const FOLLOW_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);
const FOLLOW_LOG_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// Runs the follower until the state diverges from the network's.
pub fn follow(
    store: std::sync::Arc<StateStore>,
    chain: &SharedChainStore,
    groups: u32,
) -> Result<(), String> {
    let mut follower = Follower::new(store, groups);
    let mut next_log = Instant::now() + FOLLOW_LOG_INTERVAL;
    loop {
        {
            let guard = chain.read().unwrap_or_else(PoisonError::into_inner);
            if let Some(store) = guard.as_ref() {
                follower.step(store)?;
            }
        }
        if Instant::now() >= next_log {
            next_log = Instant::now() + FOLLOW_LOG_INTERVAL;
            println!(
                "state: {} blocks applied, {} depStateHash verified",
                follower.applied, follower.verified
            );
        }
        std::thread::sleep(FOLLOW_INTERVAL);
    }
}
