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
/// `official_db` into the store at `state_path`; the database is read as a
/// secondary instance, which keeps its own files beside the store.
pub fn bootstrap(
    official_db: &Path,
    state_path: &Path,
    chain: &SharedChainStore,
    groups: u32,
) -> Result<(), String> {
    let store = StateStore::open(state_path)?;
    let official = Official::open(official_db, &state_path.with_extension("secondary"))?;
    for group in 0..groups {
        let (block, child, dep_state_hash) = settled_pair(chain, group, groups)?;
        let started = Instant::now();
        let (roots, copied) = copy_state(&official, &store, &block)?;
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
