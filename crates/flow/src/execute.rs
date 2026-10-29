//! The Danube state transition of an intra-group block
//! (`getMutableGroupViewDanube`, the scripts run by `checkTxScript`, then
//! `BlockFlowState.updateState`): the blocks its view adds, its transaction
//! scripts in block order, then its asset inputs and outputs.

use std::collections::HashMap;
use std::sync::Arc;

use alephium_chain::ChainStore;
use alephium_model::address::{lockup_group, output_hint, output_key};
use alephium_model::{
    AssetOutput, Block, BlockHeader, Hash, OutputRef, Transaction, TxOutput, deserialize,
};
use alephium_state::WorldState;
use alephium_state::trie::NodeSource;
use alephium_vm::{BlockEnv, Execution, run_tx_script};

/// Out-blocks with transactions whose inputs were already spent, which the
/// block applying them skips (`conflictedTxs`).
pub type Conflicts = Vec<(Hash, Vec<Hash>)>;

/// What applying a block produced besides the world state.
#[derive(Debug, Default)]
pub struct Applied {
    pub conflicts: Conflicts,
    /// Per transaction index, the replay of its script.
    pub executions: Vec<(usize, Execution)>,
}

/// Applies to `world`, the world state of `block`'s intra-group
/// dependency, the blocks its view adds and then `block` itself.
/// `in_conflicted(block, tx)` says whether a transaction of an incoming
/// block was skipped where it came from.
pub fn apply_block<S: NodeSource>(
    chain: &ChainStore,
    world: &mut WorldState<'_, S>,
    block: &Block,
    groups: u32,
    in_conflicted: impl Fn(&Hash, &Hash) -> bool,
) -> Result<Applied, String> {
    let hash = block.hash();
    let (group, to) = BlockHeader::chain_index(&hash, groups);
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

    let mut applied = Applied::default();
    for other in &diff {
        let other_hash = other.hash();
        let (from, to) = BlockHeader::chain_index(&other_hash, groups);
        if to == group && from != group {
            // `updateStateForInBlock`.
            for tx in &other.transactions {
                if !in_conflicted(&other_hash, &tx.id()) {
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
                applied.conflicts.push((other_hash, conflicted));
            }
        }
    }

    // `checkTxScript`, in block order since Leman. Since Rhone a transaction
    // may spend what an earlier one of the block created.
    let env = BlockEnv {
        group,
        groups,
        timestamp: block.header.timestamp,
        target: block.header.target.value(),
        block_hash: hash,
    };
    let mut created: HashMap<OutputRef, AssetOutput> = HashMap::new();
    for (index, tx) in block.transactions.iter().enumerate() {
        if let Some(script) = &tx.unsigned.script
            && tx.script_execution_ok
        {
            let prev_outputs = tx
                .unsigned
                .inputs
                .iter()
                .map(|input| asset_output(world, &created, &input.output_ref))
                .collect::<Result<Vec<_>, _>>()?;
            let execution = run_tx_script(world, &env, tx, &prev_outputs, script)
                .map_err(|failure| format!("transaction {}: {}", tx.id().to_hex(), failure.0))?;
            if execution.contract_inputs != tx.contract_inputs
                || execution.generated_outputs != tx.generated_outputs
            {
                return Err(format!(
                    "transaction {}: the replay gave contract inputs {:?} and outputs {:?}, the block {:?} and {:?}",
                    tx.id().to_hex(),
                    execution.contract_inputs,
                    execution.generated_outputs,
                    tx.contract_inputs,
                    tx.generated_outputs
                ));
            }
            applied.executions.push((index, execution));
        }
        let id = tx.id();
        for (output_index, output) in tx.unsigned.fixed_outputs.iter().enumerate() {
            created.insert(
                OutputRef {
                    hint: output_hint(&output.lockup_script),
                    key: output_key(&id, output_index_of(output_index)),
                },
                output.clone(),
            );
        }
    }

    // `updateStateForInOutBlock`.
    for tx in &block.transactions {
        remove_inputs(world, tx)?;
        add_outputs(world, tx, block, group, groups)?;
    }
    Ok(applied)
}

/// The asset output an input spends: in the world state, or created by an
/// earlier transaction of the block.
fn asset_output<S: NodeSource>(
    world: &WorldState<'_, S>,
    created: &HashMap<OutputRef, AssetOutput>,
    output_ref: &OutputRef,
) -> Result<AssetOutput, String> {
    if let Some(output) = created.get(output_ref) {
        return Ok(output.clone());
    }
    let bytes = world
        .outputs
        .get(&alephium_model::serialize(output_ref))?
        .ok_or_else(|| format!("missing input {}", output_ref.key.to_hex()))?;
    match deserialize::<TxOutput>(&bytes).map_err(|error| error.to_string())? {
        TxOutput::Asset(output) => Ok(output),
        TxOutput::Contract(_) => Err("an input spends a contract output".to_owned()),
    }
}

fn output_index_of(index: usize) -> i32 {
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
    let fixed = tx.unsigned.fixed_outputs.len();
    let all = tx.unsigned.fixed_outputs.iter().enumerate().chain(
        tx.generated_outputs
            .iter()
            .enumerate()
            .filter_map(|(offset, output)| match output {
                TxOutput::Asset(asset) => Some((fixed + offset, asset)),
                TxOutput::Contract(_) => None,
            }),
    );
    for (index, output) in all {
        if lockup_group(&output.lockup_script, groups) != group {
            continue;
        }
        let mut output = output.clone();
        if output.lock_time < block.header.timestamp {
            output.lock_time = block.header.timestamp;
        }
        let output_ref = OutputRef {
            hint: output_hint(&output.lockup_script),
            key: output_key(&id, output_index_of(index)),
        };
        world.put_output(&output_ref, &TxOutput::Asset(output))?;
    }
    Ok(())
}
