//! `GET /addresses/{address}/utxos` and `/balance` for asset addresses
//! (`TxUtils.getUTXOs`, `getBalance`): the outputs of the address in the
//! state of its group's best intra-group block, minus those spent and plus
//! those created by the newer blocks of the group's view, then by the
//! group's mempool. Deferred while that state is not followed.

use std::collections::HashSet;
use std::sync::PoisonError;

use alephium_model::address::{base58_decode, lockup_group, output_hint, output_key};
use alephium_model::hash::hex;
use alephium_model::{
    AssetOutput, BlockHeader, Hash, LockupScript, OutputRef, TxOutput, U256, deserialize,
};
use alephium_state::WorldState;
use serde_json::{Value, json};

use super::Context;

/// `api.default-utxos-limit`: more and the official node refuses.
const UTXOS_LIMIT: usize = 5000;

/// The asset outputs of `address` as the official node lists them, with
/// the mempool or without.
fn utxos(
    address: &str,
    include_pool: bool,
    context: &Context<'_>,
) -> Option<Vec<(OutputRef, AssetOutput)>> {
    let bytes = base58_decode(address)?;
    let lockup = deserialize::<LockupScript>(&bytes).ok()?;
    if !matches!(
        lockup,
        LockupScript::P2pkh(_) | LockupScript::P2mpkh { .. } | LockupScript::P2sh(_)
    ) {
        return None;
    }
    let groups = context.groups;
    let group = lockup_group(&lockup, groups);
    let (checkpoint, caches) = {
        let store = context
            .chain?
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let store = store.as_ref()?;
        if !store.weights_known(group, group) {
            return None;
        }
        let deps = store.best_deps(group, group).ok()?;
        let checkpoint = deps[(groups - 1 + group) as usize];
        let caches: Vec<_> = store
            .hashes_for_updates(group, &deps)
            .ok()?
            .iter()
            .map(|hash| store.get(hash).map(|entry| entry.block))
            .collect::<Option<_>>()?;
        (checkpoint, caches)
    };
    let states = context.state?;
    let roots = states.roots(&checkpoint).ok()??;
    let world = WorldState::new(&roots, states);
    let hint = output_hint(&lockup);
    let mut found = Vec::new();
    for (key, value) in world.outputs.get_all(&hint.to_be_bytes()).ok()? {
        let output_ref = deserialize::<OutputRef>(&key).ok()?;
        if let TxOutput::Asset(output) = deserialize::<TxOutput>(&value).ok()?
            && output.lockup_script == lockup
        {
            found.push((output_ref, output));
        }
    }
    if found.len() > UTXOS_LIMIT {
        return None;
    }
    // The blocks of the group's view that the state has not applied.
    let mut spent: HashSet<OutputRef> = HashSet::new();
    let mut created = Vec::new();
    for block in &caches {
        let (from, to) = BlockHeader::chain_index(&block.hash(), groups);
        if from != group && to != group {
            continue;
        }
        for tx in &block.transactions {
            spent.extend(tx.unsigned.inputs.iter().map(|input| input.output_ref));
            spent.extend(tx.contract_inputs.iter().copied());
            created.extend(asset_outputs(
                tx.id(),
                &tx.unsigned.fixed_outputs,
                &tx.generated_outputs,
                &lockup,
            ));
        }
    }
    let mut result: Vec<(OutputRef, AssetOutput)> = found
        .into_iter()
        .filter(|(output_ref, _)| !spent.contains(output_ref))
        .chain(
            created
                .into_iter()
                .filter(|(output_ref, _)| !spent.contains(output_ref)),
        )
        .collect();
    if include_pool {
        let pool = context
            .mempool?
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut pool_spent: HashSet<OutputRef> = HashSet::new();
        for tx in pool.iter().filter(|tx| tx.chain_from == group) {
            let unsigned = &tx.template.unsigned;
            pool_spent.extend(unsigned.inputs.iter().map(|input| input.output_ref));
            for (output_ref, output) in
                asset_outputs(unsigned.id(), &unsigned.fixed_outputs, &[], &lockup)
            {
                if !result.iter().any(|(held, _)| *held == output_ref) {
                    result.push((output_ref, output));
                }
            }
        }
        result.retain(|(output_ref, _)| !pool_spent.contains(output_ref));
    }
    Some(result)
}

/// The asset outputs of a transaction locked by `lockup`, with their refs.
fn asset_outputs(
    tx_id: Hash,
    fixed: &[AssetOutput],
    generated: &[TxOutput],
    lockup: &LockupScript,
) -> Vec<(OutputRef, AssetOutput)> {
    let fixed_len = fixed.len();
    fixed
        .iter()
        .enumerate()
        .chain(
            generated
                .iter()
                .enumerate()
                .filter_map(|(offset, output)| match output {
                    TxOutput::Asset(asset) => Some((fixed_len + offset, asset)),
                    TxOutput::Contract(_) => None,
                }),
        )
        .filter(|(_, output)| output.lockup_script == *lockup)
        .map(|(index, output)| {
            let output_ref = OutputRef {
                hint: output_hint(&output.lockup_script),
                key: output_key(&tx_id, i32::try_from(index).unwrap_or(i32::MAX)),
            };
            (output_ref, output.clone())
        })
        .collect()
}

fn tokens_json(tokens: &[(Hash, U256)]) -> Value {
    Value::Array(
        tokens
            .iter()
            .map(|(id, amount)| json!({"id": id.to_hex(), "amount": amount.to_string()}))
            .collect(),
    )
}

pub(super) fn utxos_json(address: &str, context: &Context<'_>) -> Option<Vec<u8>> {
    let utxos = utxos(address, true, context)?;
    let list: Vec<Value> = utxos
        .iter()
        .map(|(output_ref, output)| {
            let mut utxo = json!({
                "ref": {"hint": output_ref.hint, "key": output_ref.key.to_hex()},
                "amount": output.amount.to_string(),
            });
            if !output.tokens.is_empty() {
                utxo["tokens"] = tokens_json(&output.tokens);
            }
            utxo["lockTime"] = json!(output.lock_time.0);
            utxo["additionalData"] = json!(hex(&output.additional_data));
            utxo
        })
        .collect();
    Some(json!({"utxos": list}).to_string().into_bytes())
}

/// `Amount.hint`: ALPH with its decimals, trailing zeros dropped.
fn alph_hint(amount: U256) -> String {
    let one = U256::from(1_000_000_000_000_000_000u64);
    let (whole, fraction) = (amount / one, amount % one);
    if fraction == U256::ZERO {
        format!("{whole} ALPH")
    } else {
        let digits = format!("{fraction:0>18}");
        format!("{whole}.{} ALPH", digits.trim_end_matches('0'))
    }
}

pub(super) fn balance_json(
    address: &str,
    query: Option<&str>,
    context: &Context<'_>,
) -> Option<Vec<u8>> {
    let include_pool = match query {
        None => false,
        Some("mempool=true") => true,
        Some("mempool=false") => false,
        Some(_) => return None,
    };
    let utxos = utxos(address, include_pool, context)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    let (mut total, mut locked) = (U256::ZERO, U256::ZERO);
    let mut tokens: Vec<(Hash, U256)> = Vec::new();
    let mut locked_tokens: Vec<(Hash, U256)> = Vec::new();
    let add = |list: &mut Vec<(Hash, U256)>, id: Hash, amount: U256| match list
        .iter_mut()
        .find(|(held, _)| *held == id)
    {
        Some((_, held)) => *held = held.saturating_add(amount),
        None => list.push((id, amount)),
    };
    for (_, output) in &utxos {
        total = total.saturating_add(output.amount);
        let is_locked = u128::from(output.lock_time.0) > now;
        if is_locked {
            locked = locked.saturating_add(output.amount);
        }
        for (id, amount) in &output.tokens {
            add(&mut tokens, *id, *amount);
            if is_locked {
                add(&mut locked_tokens, *id, *amount);
            }
        }
    }
    let mut body = json!({
        "balance": total.to_string(),
        "balanceHint": alph_hint(total),
        "lockedBalance": locked.to_string(),
        "lockedBalanceHint": alph_hint(locked),
    });
    if !tokens.is_empty() {
        body["tokenBalances"] = tokens_json(&tokens);
    }
    if !locked_tokens.is_empty() {
        body["lockedTokenBalances"] = tokens_json(&locked_tokens);
    }
    body["utxoNum"] = json!(utxos.len());
    Some(body.to_string().into_bytes())
}
