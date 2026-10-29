//! `GET /transactions/status`: a transaction confirmed on an intra-group
//! chain is answered from the followed chain (`TxUtils.getTxConfirmationStatus`
//! searched like `EndpointsLogic.searchTransactionStatus`). Everything else is
//! deferred: `MemPooled` and `TxNotFound` need the native mempool, and an
//! inter-group block can hold `Conflicted` transactions, which need state.

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
    // `brokerConfig.chainIndexes` order, filtered by the given groups.
    for chain_from in (0..context.groups).filter(|g| from.is_none_or(|from| from == *g)) {
        for chain_to in (0..context.groups).filter(|g| to.is_none_or(|to| to == *g)) {
            let Some(confirmed) = store.tx_confirmed(&tx, chain_from, chain_to) else {
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
    None
}
