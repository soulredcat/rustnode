//! `GET /contracts/{address}/state` (`ServerUtils.getContractState`): the
//! contract in the world state of its group's best intra-group tip
//! (`getBestCachedWorldState`), read from the followed state. Deferred while
//! the follower has not reached that tip.

use std::sync::PoisonError;

use alephium_model::address::{address_string, base58_decode};
use alephium_model::hash::hex;
use alephium_model::{Hash, LockupScript, TxOutput, Val, deserialize, serialize};
use alephium_state::WorldState;
use alephium_vm::contract::{CODE_TAG, Code, IMMUTABLE_STATE_TAG, ImmutableState, StoredState};
use serde_json::{Value, json};

use super::Context;

/// `Address.Contract`: prefix 3, then the contract id.
const CONTRACT_PREFIX: u8 = 3;

pub(super) fn state(address: &str, context: &Context<'_>) -> Option<Vec<u8>> {
    let bytes = base58_decode(address)?;
    let (&prefix, id) = bytes.split_first()?;
    if prefix != CONTRACT_PREFIX || id.len() != 32 {
        return None;
    }
    let mut raw = [0u8; 32];
    raw.copy_from_slice(id);
    let id = Hash(raw);
    let group = u32::from(raw[31]);
    if group >= context.groups {
        return None;
    }
    let tip = {
        let store = context
            .chain?
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let store = store.as_ref()?;
        if !store.weights_known(group, group) {
            return None;
        }
        store.best_deps(group, group).ok()?[(context.groups - 1 + group) as usize]
    };
    let states = context.state?;
    let roots = states.roots(&tip).ok()??;
    let world = WorldState::new(&roots, states);
    let stored = StoredState::decode(&world.contracts.get(&id.0).ok()??).ok()?;
    let (code, initial_state_hash, imm_fields, mut_fields) = match stored.clone() {
        StoredState::Mutable {
            mut_fields,
            immutable_hash,
            ..
        } => {
            let immutable =
                ImmutableState::decode(&tagged(&world, &immutable_hash, IMMUTABLE_STATE_TAG)?)
                    .ok()?;
            let code = Code::decode(&tagged(&world, &immutable.code_hash, CODE_TAG)?).ok()?;
            (
                code,
                immutable.initial_state_hash,
                immutable.imm_fields,
                mut_fields,
            )
        }
        // Contracts from before Leman keep their code in the code trie;
        // deferred until one is met.
        StoredState::Legacy { .. } => return None,
    };
    let output =
        deserialize::<TxOutput>(&world.outputs.get(&serialize(stored.output_ref())).ok()??).ok()?;
    let TxOutput::Contract(output) = output else {
        return None;
    };
    let body = json!({
        "address": address_string(&LockupScript::P2c(id)),
        "bytecode": hex(&code.encode()),
        "codeHash": code.hash.to_hex(),
        "initialStateHash": initial_state_hash.to_hex(),
        "immFields": imm_fields.iter().map(val_json).collect::<Vec<_>>(),
        "mutFields": mut_fields.iter().map(val_json).collect::<Vec<_>>(),
        "asset": {
            "attoAlphAmount": output.amount.to_string(),
            "tokens": output.tokens.iter().map(|(token, amount)| json!({
                "id": token.to_hex(),
                "amount": amount.to_string(),
            })).collect::<Vec<_>>(),
        },
    });
    Some(body.to_string().into_bytes())
}

fn tagged<S: alephium_state::trie::NodeSource>(
    world: &WorldState<'_, S>,
    hash: &Hash,
    tag: u8,
) -> Option<Vec<u8>> {
    let bytes = world.immutable(hash).ok()??;
    let (first, rest) = bytes.split_first()?;
    (*first == tag).then(|| rest.to_vec())
}

/// `api.model.Val`.
pub(crate) fn val_json(value: &Val) -> Value {
    match value {
        Val::Bool(value) => json!({"type": "Bool", "value": value}),
        Val::I256(value) => json!({"type": "I256", "value": value.to_string()}),
        Val::U256(value) => json!({"type": "U256", "value": value.to_string()}),
        Val::ByteVec(bytes) => json!({"type": "ByteVec", "value": hex(bytes)}),
        Val::Address(lockup) => json!({"type": "Address", "value": address_string(lockup)}),
    }
}
