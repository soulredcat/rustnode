//! `GET /mempool/transactions` (`EndpointsLogic.listMempoolTransactions`):
//! per group, its transactions grouped by chain index. The mempool fills
//! from what the peers relay, so it is served only while a chain is
//! followed.

use std::sync::PoisonError;

use serde_json::{Value, json};

use super::Context;

pub(super) fn transactions(context: &Context<'_>) -> Option<Vec<u8>> {
    let store = context
        .chain?
        .read()
        .unwrap_or_else(PoisonError::into_inner);
    store.as_ref()?;
    let pool = context
        .mempool?
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    // Chain indexes in order of their first transaction, per group.
    let mut chains: Vec<((u32, u32), Vec<Value>)> = Vec::new();
    for group in 0..context.groups {
        for tx in pool.iter().filter(|tx| tx.chain_from == group) {
            let index = (tx.chain_from, tx.chain_to);
            let json = crate::json::transaction_template(&tx.template, tx.seen_at);
            match chains.iter_mut().find(|(held, _)| *held == index) {
                Some((_, txs)) => txs.push(json),
                None => chains.push((index, vec![json])),
            }
        }
    }
    let body: Vec<Value> = chains
        .into_iter()
        .map(|((from, to), transactions)| {
            json!({"fromGroup": from, "toGroup": to, "transactions": transactions})
        })
        .collect();
    Some(Value::Array(body).to_string().into_bytes())
}
