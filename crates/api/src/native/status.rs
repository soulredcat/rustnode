//! `GET /transactions/status`, searched like
//! `EndpointsLogic.searchTransactionStatus`: per chain index, a transaction
//! confirmed on it (`TxUtils.getTxConfirmedStatus`), else one in the mempool
//! of its group (`MemPooled`); `TxNotFound` when no chain index has it. A
//! transaction confirmed on an inter-group chain is deferred: such a block can
//! hold `Conflicted` transactions, which need state. So is every request while
//! a searched chain has a tip of unknown weight (before the peer's
//! `ChainState`), as its canonical blocks are not known yet. The followed
//! window is all rustnode keeps, so a transaction confirmed before it is not
//! found.

use std::sync::PoisonError;

use alephium_model::Hash;
use serde_json::json;

use super::Context;

fn group(value: &str, groups: u32) -> Option<u32> {
    value.parse::<u32>().ok().filter(|group| *group < groups)
}

pub(super) fn transaction_status(query: Option<&str>, context: &Context<'_>) -> Option<Vec<u8>> {
    let (mut tx, mut from, mut to) = (None, None, None);
    for pair in query?.split('&') {
        let (key, value) = pair.split_once('=')?;
        match key {
            "txId" => tx = Some(Hash::from_hex(value)?),
            "fromGroup" => from = Some(group(value, context.groups)?),
            "toGroup" => to = Some(group(value, context.groups)?),
            _ => return None,
        }
    }
    let tx = tx?;
    let store = context
        .chain?
        .read()
        .unwrap_or_else(PoisonError::into_inner);
    let store = store.as_ref()?;
    let searched = |chain_from: u32, chain_to: u32| {
        from.is_none_or(|from| from == chain_from) && to.is_none_or(|to| to == chain_to)
    };
    for chain_from in 0..context.groups {
        for chain_to in 0..context.groups {
            if searched(chain_from, chain_to) && !store.weights_known(chain_from, chain_to) {
                return None;
            }
        }
    }
    let pool = context
        .mempool?
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    // `brokerConfig.chainIndexes` order, filtered by the given groups.
    for chain_from in (0..context.groups).filter(|g| from.is_none_or(|from| from == *g)) {
        for chain_to in (0..context.groups).filter(|g| to.is_none_or(|to| to == *g)) {
            let Some(confirmed) = store.tx_confirmed(&tx, chain_from, chain_to) else {
                if pool.holds(&tx, chain_from) {
                    return Some(json!({"type": "MemPooled"}).to_string().into_bytes());
                }
                continue;
            };
            if chain_from != chain_to {
                return None;
            }
            let confirmations = confirmed.chain_confirmations;
            return Some(
                json!({
                    "type": "Confirmed",
                    "blockHash": confirmed.block.to_hex(),
                    "txIndex": confirmed.index,
                    "chainConfirmations": confirmations,
                    "fromGroupConfirmations": confirmations,
                    "toGroupConfirmations": confirmations,
                })
                .to_string()
                .into_bytes(),
            );
        }
    }
    Some(json!({"type": "TxNotFound"}).to_string().into_bytes())
}
