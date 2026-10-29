//! The Danube state transition of an intra-group block
//! (`getMutableGroupViewDanube`, then `BlockFlowState.updateState`), for
//! what the blocks themselves record: asset outputs, and the contract
//! outputs a block's scripts spent and created. Contract fields need the VM.

use std::sync::Arc;

use alephium_chain::ChainStore;
use alephium_model::address::{lockup_group, output_hint, output_key};
use alephium_model::{Block, BlockHeader, Hash, LockupScript, OutputRef, Transaction, TxOutput};
use alephium_state::WorldState;
use alephium_state::trie::NodeSource;

/// Out-blocks with transactions whose inputs were already spent, which the
/// block applying them skips (`conflictedTxs`).
pub type Conflicts = Vec<(Hash, Vec<Hash>)>;

/// Applies to `world`, the world state of `block`'s intra-group
/// dependency, the blocks its view adds and then `block` itself.
/// `in_conflicted(block, tx)` says whether a transaction of an incoming
/// block was skipped where it came from.
pub fn apply_outputs<S: NodeSource>(
    chain: &ChainStore,
    world: &mut WorldState<'_, S>,
    block: &Block,
    groups: u32,
    in_conflicted: impl Fn(&Hash, &Hash) -> bool,
) -> Result<Conflicts, String> {
    let (group, to) = BlockHeader::chain_index(&block.hash(), groups);
    if group != to {
        return Err("only intra-group blocks carry a world state".to_owned());
    }
    let mut diff: Vec<Arc<Block>> = chain
        .hashes_for_updates(group, &block.header.deps)
        .map_err(|error| format!("{error:?}"))?
        .iter()
        .map(|hash| {
            chain
                .get(hash)
                .map(|entry| Arc::clone(&entry.block))
                .ok_or_else(|| format!("missing block {}", hash.to_hex()))
        })
        .collect::<Result<_, _>>()?;
    diff.sort_by_key(|block| block.header.timestamp);

    let mut conflicts = Conflicts::new();
    for other in &diff {
        let hash = other.hash();
        let (from, to) = BlockHeader::chain_index(&hash, groups);
        if to == group && from != group {
            // `updateStateForInBlock`.
            for tx in &other.transactions {
                if !in_conflicted(&hash, &tx.id()) {
                    add_outputs(world, tx, other, group, groups)?;
                }
            }
        } else if from == group {
            // `updateStateForOutBlock`: inputs must still be unspent.
            let mut conflicted = Vec::new();
            for tx in &other.transactions {
                let mut unspent = true;
                for input in &tx.unsigned.inputs {
                    if !world.has_output(&input.output_ref)? {
                        unspent = false;
                        break;
                    }
                }
                if unspent {
                    remove_inputs(world, tx)?;
                    add_outputs(world, tx, other, group, groups)?;
                } else {
                    conflicted.push(tx.id());
                }
            }
            if !conflicted.is_empty() {
                conflicts.push((hash, conflicted));
            }
        }
    }

    for tx in &block.transactions {
        // What the scripts did to contract outputs.
        for input in &tx.contract_inputs {
            world.remove_output(input)?;
        }
        let id = tx.id();
        let fixed = tx.unsigned.fixed_outputs.len();
        for (offset, output) in tx.generated_outputs.iter().enumerate() {
            if let TxOutput::Contract(contract) = output {
                let output_ref = OutputRef {
                    hint: output_hint(&LockupScript::P2c(contract.contract_id)),
                    key: output_key(&id, output_index(fixed + offset)),
                };
                world.put_output(&output_ref, output)?;
            }
        }
        // `updateStateForInOutBlock`.
        remove_inputs(world, tx)?;
        add_outputs(world, tx, block, group, groups)?;
    }
    Ok(conflicts)
}

fn output_index(index: usize) -> i32 {
    i32::try_from(index).unwrap_or(i32::MAX)
}

/// `updateStateForInputs`.
fn remove_inputs<S: NodeSource>(
    world: &mut WorldState<'_, S>,
    tx: &Transaction,
) -> Result<(), String> {
    for input in &tx.unsigned.inputs {
        world.remove_output(&input.output_ref)?;
    }
    Ok(())
}

/// `updateStateForOutputs`: the asset outputs to `group`, locked at least
/// until the block's time.
fn add_outputs<S: NodeSource>(
    world: &mut WorldState<'_, S>,
    tx: &Transaction,
    block: &Block,
    group: u32,
    groups: u32,
) -> Result<(), String> {
    let id = tx.id();
    let fixed = tx.unsigned.fixed_outputs.iter();
    let generated = tx
        .generated_outputs
        .iter()
        .filter_map(|output| match output {
            TxOutput::Asset(asset) => Some(asset),
            TxOutput::Contract(_) => None,
        });
    let indexes = (0..tx.unsigned.fixed_outputs.len()).chain(
        tx.generated_outputs
            .iter()
            .enumerate()
            .filter(|(_, output)| matches!(output, TxOutput::Asset(_)))
            .map(|(offset, _)| tx.unsigned.fixed_outputs.len() + offset),
    );
    for (index, output) in indexes.zip(fixed.chain(generated)) {
        if lockup_group(&output.lockup_script, groups) != group {
            continue;
        }
        let mut output = output.clone();
        if output.lock_time < block.header.timestamp {
            output.lock_time = block.header.timestamp;
        }
        let output_ref = OutputRef {
            hint: output_hint(&output.lockup_script),
            key: output_key(&id, output_index(index)),
        };
        world.put_output(&output_ref, &TxOutput::Asset(output))?;
    }
    Ok(())
}
