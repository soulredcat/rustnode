//! Contract events and sub-contracts (`ServerUtils.getEventsByContractAddress`,
//! `getEventsForContractCurrentCount`, `getEventsByTxId`,
//! `getEventsByBlockHash`, `getSubContracts`), read from the event pages the
//! state store keeps like the official node's `LogStorage`.

use std::sync::PoisonError;

use alephium_model::address::{address_string, base58_decode};
use alephium_model::{BlockHeader, Hash, LockupScript};
use alephium_state::{LogState, LogStates};
use serde_json::{Value, json};

use super::Context;
use super::contract::val_json;

/// `CounterRange.MaxCounterRange`, also the default limit.
const MAX_COUNTER_RANGE: i32 = 100;
/// `debugEventIndex`: never listed.
const DEBUG_EVENT_INDEX: u8 = 0xfd;

fn contract_id(address: &str) -> Option<Hash> {
    let bytes = base58_decode(address)?;
    let (&prefix, id) = bytes.split_first()?;
    if prefix != 3 || id.len() != 32 {
        return None;
    }
    let mut raw = [0u8; 32];
    raw.copy_from_slice(id);
    Some(Hash(raw))
}

/// `start` (required) and `limit` (1 to 100, default 100); `group` is
/// accepted and has no effect with one broker.
fn counter_range(query: Option<&str>) -> Option<(i32, i32)> {
    let (mut start, mut limit) = (None, MAX_COUNTER_RANGE);
    for pair in query?.split('&') {
        let (key, value) = pair.split_once('=')?;
        match key {
            "start" => start = Some(value.parse::<i32>().ok().filter(|start| *start >= 0)?),
            "limit" => {
                limit = value
                    .parse::<i32>()
                    .ok()
                    .filter(|limit| (1..=MAX_COUNTER_RANGE).contains(limit))?;
            }
            "group" => {}
            _ => return None,
        }
    }
    let start = start?;
    start.checked_add(limit)?;
    Some((start, limit))
}

fn contract_address(id: &Hash) -> String {
    address_string(&LockupScript::P2c(*id))
}

fn fields(state: &LogState) -> Value {
    Value::Array(state.fields.iter().map(val_json).collect())
}

fn event_index(state: &LogState) -> i32 {
    i32::from(i8::from_be_bytes([state.index]))
}

pub(super) fn current_count(address: &str, context: &Context<'_>) -> Option<Vec<u8>> {
    let count = context.state?.event_count(&contract_id(address)?).ok()??;
    Some(count.to_string().into_bytes())
}

pub(super) fn contract_events(
    address: &str,
    query: Option<&str>,
    context: &Context<'_>,
) -> Option<Vec<u8>> {
    let id = contract_id(address)?;
    let (start, limit) = counter_range(query)?;
    let store = context.state?;
    let mut pages: Vec<LogStates> = Vec::new();
    let mut next = start;
    while next < start + limit {
        let Some(page) = store.event_page(&id, next).ok()? else {
            break;
        };
        pages.push(page);
        next += 1;
    }
    if pages.is_empty() {
        let count = store.event_count(&id).ok()??;
        return (count == start).then(|| {
            json!({"events": [], "nextStart": start})
                .to_string()
                .into_bytes()
        });
    }
    let mut events = Vec::new();
    for page in &pages {
        let timestamp = store.block_time(&page.block).ok()??;
        for state in page
            .states
            .iter()
            .filter(|state| state.index != DEBUG_EVENT_INDEX)
        {
            events.push(json!({
                "blockHash": page.block.to_hex(),
                "txId": state.tx_id.to_hex(),
                "timestamp": timestamp,
                "contractAddress": contract_address(&page.contract),
                "eventIndex": event_index(state),
                "fields": fields(state),
            }));
        }
    }
    Some(
        json!({"events": events, "nextStart": next})
            .to_string()
            .into_bytes(),
    )
}

/// Whether `block` is on its chain's canonical branch, `None` when it is
/// outside the window.
fn canonical(context: &Context<'_>, block: &Hash) -> Option<bool> {
    let store = context
        .chain?
        .read()
        .unwrap_or_else(PoisonError::into_inner);
    let store = store.as_ref()?;
    let entry = store.get(block)?;
    Some(store.hashes_at(entry.from, entry.to, entry.height).first() == Some(block))
}

pub(super) fn events_by_tx_id(tx_id: &str, context: &Context<'_>) -> Option<Vec<u8>> {
    let tx_id = Hash::from_hex(tx_id)?;
    let store = context.state?;
    let events = store.events_by_hash(&tx_id).ok()?;
    let several_blocks = events
        .windows(2)
        .any(|pair| pair[0].0.block != pair[1].0.block);
    let mut listed = Vec::new();
    for (page, state) in &events {
        // `eventsFromCanonicalChain`: of a transaction in several blocks,
        // the canonical block's events.
        if several_blocks && !canonical(context, &page.block)? {
            continue;
        }
        listed.push(json!({
            "blockHash": page.block.to_hex(),
            "timestamp": store.block_time(&page.block).ok()??,
            "contractAddress": contract_address(&page.contract),
            "eventIndex": event_index(state),
            "fields": fields(state),
        }));
    }
    Some(json!({"events": listed}).to_string().into_bytes())
}

/// An intra-group block the state follower has applied, with its events.
pub(super) fn block_with_events(hash: &str, context: &Context<'_>) -> Option<Vec<u8>> {
    let hash = Hash::from_hex(hash)?;
    let store = context.state?;
    store.roots(&hash).ok()??;
    let (block, height) = {
        let chain = context
            .chain?
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let entry = chain.as_ref()?.get(&hash)?;
        (entry.block, entry.height)
    };
    let (from, to) = BlockHeader::chain_index(&hash, context.groups);
    if from != to {
        return None;
    }
    let block_json = crate::json::block_entry(&block, height, context.groups, None)?;
    let timestamp = block.header.timestamp.0;
    let events: Vec<Value> = store
        .events_by_hash(&hash)
        .ok()?
        .iter()
        .map(|(page, state)| {
            json!({
                "txId": state.tx_id.to_hex(),
                "timestamp": timestamp,
                "contractAddress": contract_address(&page.contract),
                "eventIndex": event_index(state),
                "fields": fields(state),
            })
        })
        .collect();
    Some(
        json!({"block": block_json, "events": events})
            .to_string()
            .into_bytes(),
    )
}

pub(super) fn sub_contracts(
    address: &str,
    query: Option<&str>,
    context: &Context<'_>,
) -> Option<Vec<u8>> {
    let id = contract_id(address)?;
    let (start, limit) = counter_range(query)?;
    let store = context.state?;
    let mut children = Vec::new();
    let mut next = start;
    while next < start + limit {
        let Some(page) = store.sub_contract_page(&id, next).ok()? else {
            break;
        };
        children.extend(page.iter().map(contract_address));
        next += 1;
    }
    if next == start {
        let count = store.sub_contract_count(&id).ok()??;
        return (count == start).then(|| {
            json!({"subContracts": [], "nextStart": count})
                .to_string()
                .into_bytes()
        });
    }
    Some(
        json!({"subContracts": children, "nextStart": next})
            .to_string()
            .into_bytes(),
    )
}
