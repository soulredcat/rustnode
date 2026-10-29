//! Contract events and the sub-contract index, kept like the official node
//! keeps them (`LogStorage`, `SubContractIndexStorage`): per contract one
//! page of events per block, numbered by a per-contract page counter; the
//! events of each transaction and each block by reference; and per parent
//! contract one page of sub-contracts per block. The history up to the
//! state bootstrap is copied from the official node's database, the rest
//! is written as blocks are followed.

use std::collections::HashMap;

use alephium_model::{Codec, Hash, Val, deserialize, serialize};
use redb::{ReadableDatabase, ReadableTable, TableDefinition};

use crate::store::StateStore;

/// The official column families, raw: `log` holds pages under
/// `LogStatesId` keys and references under 32-byte transaction and block
/// hashes.
pub(crate) const FAMILIES: [&str; 5] = [
    "log",
    "logcounter",
    "subcontract",
    "subcontractcounter",
    "parentcontract",
];

/// Timestamps of the blocks holding event pages, which may be older than
/// the block window.
pub(crate) const BLOCK_TIMES: TableDefinition<[u8; 32], u64> = TableDefinition::new("blocktime");

fn table(name: &str) -> TableDefinition<'_, &'static [u8], &'static [u8]> {
    TableDefinition::new(name)
}

/// `LogState`: an event of a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogState {
    pub tx_id: Hash,
    pub index: u8,
    pub fields: Vec<Val>,
}

/// `LogStates`: a contract's events of one block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogStates {
    pub block: Hash,
    pub contract: Hash,
    pub states: Vec<LogState>,
}

impl LogStates {
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let (block, (contract, states)) =
            deserialize::<(Hash, (Hash, Vec<(Hash, (u8, Vec<Val>))>))>(bytes)
                .map_err(|error| error.to_string())?;
        Ok(Self {
            block,
            contract,
            states: states
                .into_iter()
                .map(|(tx_id, (index, fields))| LogState {
                    tx_id,
                    index,
                    fields,
                })
                .collect(),
        })
    }

    fn encode(&self) -> Vec<u8> {
        let states: Vec<(Hash, (u8, Vec<Val>))> = self
            .states
            .iter()
            .map(|state| (state.tx_id, (state.index, state.fields.clone())))
            .collect();
        serialize(&(self.block, (self.contract, states)))
    }
}

/// `LogStatesId` / `SubContractIndexStateId`: a contract and a page number.
fn page_id(contract: &Hash, page: i32) -> Vec<u8> {
    let mut key = contract.0.to_vec();
    page.encode(&mut key);
    key
}

/// `LogStateRef`: a page and an offset in it.
type LogRef = (Hash, (i32, i32));

/// An event to keep: from a contract (or a system event id), in a
/// transaction.
#[derive(Debug, Clone)]
pub struct NewEvent {
    pub tx_id: Hash,
    pub contract: Hash,
    pub fields: Vec<Val>,
}

impl StateStore {
    fn read_raw(&self, family: &str, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let txn = self.db().begin_read().map_err(|error| error.to_string())?;
        let table = match txn.open_table(table(family)) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        Ok(table
            .get(key)
            .map_err(|error| error.to_string())?
            .map(|value| value.value().to_vec()))
    }

    /// The events page counter of a contract (`getEventsCurrentCount`).
    pub fn event_count(&self, contract: &Hash) -> Result<Option<i32>, String> {
        self.read_raw("logcounter", &contract.0)?
            .map(|bytes| deserialize::<i32>(&bytes).map_err(|error| error.to_string()))
            .transpose()
    }

    pub fn event_page(&self, contract: &Hash, page: i32) -> Result<Option<LogStates>, String> {
        self.read_raw("log", &page_id(contract, page))?
            .map(|bytes| LogStates::decode(&bytes))
            .transpose()
    }

    /// `getEventsByHash`: the events of a transaction or a block, with the
    /// page each is in.
    pub fn events_by_hash(&self, hash: &Hash) -> Result<Vec<(LogStates, LogState)>, String> {
        let Some(refs) = self.read_raw("log", &hash.0)? else {
            return Ok(Vec::new());
        };
        let refs = deserialize::<Vec<LogRef>>(&refs).map_err(|error| error.to_string())?;
        let mut events = Vec::with_capacity(refs.len());
        for (contract, (page, offset)) in refs {
            let states = self
                .event_page(&contract, page)?
                .ok_or_else(|| format!("missing event page {page} of {}", contract.to_hex()))?;
            let state = usize::try_from(offset)
                .ok()
                .and_then(|offset| states.states.get(offset).cloned())
                .ok_or_else(|| format!("invalid event offset {offset}"))?;
            events.push((states, state));
        }
        Ok(events)
    }

    /// The timestamp of a block holding events.
    pub fn block_time(&self, block: &Hash) -> Result<Option<u64>, String> {
        let txn = self.db().begin_read().map_err(|error| error.to_string())?;
        let table = match txn.open_table(BLOCK_TIMES) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        Ok(table
            .get(block.0)
            .map_err(|error| error.to_string())?
            .map(|value| value.value()))
    }

    pub(crate) fn put_block_times(&self, times: &[(Hash, u64)]) -> Result<(), String> {
        let txn = self.db().begin_write().map_err(|error| error.to_string())?;
        {
            let mut table = txn
                .open_table(BLOCK_TIMES)
                .map_err(|error| error.to_string())?;
            for (block, time) in times {
                table
                    .insert(block.0, *time)
                    .map_err(|error| error.to_string())?;
            }
        }
        txn.commit().map_err(|error| error.to_string())
    }

    /// The sub-contracts page counter of a parent contract.
    pub fn sub_contract_count(&self, parent: &Hash) -> Result<Option<i32>, String> {
        self.read_raw("subcontractcounter", &parent.0)?
            .map(|bytes| deserialize::<i32>(&bytes).map_err(|error| error.to_string()))
            .transpose()
    }

    pub fn sub_contract_page(&self, parent: &Hash, page: i32) -> Result<Option<Vec<Hash>>, String> {
        self.read_raw("subcontract", &page_id(parent, page))?
            .map(|bytes| deserialize::<Vec<Hash>>(&bytes).map_err(|error| error.to_string()))
            .transpose()
    }

    /// `putLog` and `createSubContractIndexes` for one block: one page per
    /// contract, the counters one up, and the references by transaction and
    /// by block. A block whose events the copied history already holds is
    /// skipped.
    pub fn put_block_events(
        &self,
        block: &Hash,
        timestamp: u64,
        events: &[NewEvent],
        sub_contracts: &[(Hash, Hash)],
    ) -> Result<(), String> {
        if (events.is_empty() && sub_contracts.is_empty())
            || self.read_raw("log", &block.0)?.is_some()
        {
            return Ok(());
        }
        let txn = self.db().begin_write().map_err(|error| error.to_string())?;
        txn.open_table(BLOCK_TIMES)
            .map_err(|error| error.to_string())?
            .insert(block.0, timestamp)
            .map_err(|error| error.to_string())?;
        {
            let mut log = txn
                .open_table(table("log"))
                .map_err(|error| error.to_string())?;
            let mut counters = txn
                .open_table(table("logcounter"))
                .map_err(|error| error.to_string())?;
            let mut pages: HashMap<Hash, (i32, LogStates)> = HashMap::new();
            let mut by_hash: Vec<(Hash, LogRef)> = Vec::new();
            for event in events {
                // `getEventIndex`: the first field, an I256 that fits a byte.
                let Some(Val::I256(index)) = event.fields.first() else {
                    continue;
                };
                let Ok(index) = i8::try_from(*index) else {
                    continue;
                };
                let (page, states) = match pages.entry(event.contract) {
                    std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        let count = match counters
                            .get(&event.contract.0[..])
                            .map_err(|error| error.to_string())?
                        {
                            Some(bytes) => deserialize::<i32>(bytes.value())
                                .map_err(|error| error.to_string())?,
                            None => 0,
                        };
                        entry.insert((
                            count,
                            LogStates {
                                block: *block,
                                contract: event.contract,
                                states: Vec::new(),
                            },
                        ))
                    }
                };
                let offset = i32::try_from(states.states.len()).map_err(|_| "offset")?;
                states.states.push(LogState {
                    tx_id: event.tx_id,
                    index: index.to_be_bytes()[0],
                    fields: event.fields[1..].to_vec(),
                });
                for hash in [event.tx_id, *block] {
                    by_hash.push((hash, (event.contract, (*page, offset))));
                }
            }
            for (contract, (page, states)) in &pages {
                log.insert(&page_id(contract, *page)[..], &states.encode()[..])
                    .map_err(|error| error.to_string())?;
                counters
                    .insert(&contract.0[..], &serialize(&(page + 1))[..])
                    .map_err(|error| error.to_string())?;
            }
            for (hash, reference) in by_hash {
                let mut refs = match log.get(&hash.0[..]).map_err(|error| error.to_string())? {
                    Some(bytes) => deserialize::<Vec<LogRef>>(bytes.value())
                        .map_err(|error| error.to_string())?,
                    None => Vec::new(),
                };
                refs.push(reference);
                log.insert(&hash.0[..], &serialize(&refs)[..])
                    .map_err(|error| error.to_string())?;
            }
        }
        if !sub_contracts.is_empty() {
            let mut parents = txn
                .open_table(table("parentcontract"))
                .map_err(|error| error.to_string())?;
            let mut subs = txn
                .open_table(table("subcontract"))
                .map_err(|error| error.to_string())?;
            let mut counters = txn
                .open_table(table("subcontractcounter"))
                .map_err(|error| error.to_string())?;
            let mut initial: HashMap<Hash, i32> = HashMap::new();
            for (parent, child) in sub_contracts {
                parents
                    .insert(&child.0[..], &parent.0[..])
                    .map_err(|error| error.to_string())?;
                let page = match initial.get(parent) {
                    Some(page) => *page,
                    None => {
                        let count = match counters
                            .get(&parent.0[..])
                            .map_err(|error| error.to_string())?
                        {
                            Some(bytes) => deserialize::<i32>(bytes.value())
                                .map_err(|error| error.to_string())?,
                            None => 0,
                        };
                        initial.insert(*parent, count);
                        count
                    }
                };
                let key = page_id(parent, page);
                let mut children = match subs.get(&key[..]).map_err(|error| error.to_string())? {
                    Some(bytes) => deserialize::<Vec<Hash>>(bytes.value())
                        .map_err(|error| error.to_string())?,
                    None => Vec::new(),
                };
                children.push(*child);
                subs.insert(&key[..], &serialize(&children)[..])
                    .map_err(|error| error.to_string())?;
                counters
                    .insert(&parent.0[..], &serialize(&(page + 1))[..])
                    .map_err(|error| error.to_string())?;
            }
        }
        txn.commit().map_err(|error| error.to_string())
    }

    /// Writes raw entries of an official column family.
    pub(crate) fn put_raw(
        &self,
        family: &str,
        entries: &[(Vec<u8>, Vec<u8>)],
    ) -> Result<(), String> {
        let mut txn = self.db().begin_write().map_err(|error| error.to_string())?;
        txn.set_durability(redb::Durability::None)
            .map_err(|error| error.to_string())?;
        {
            let mut table = txn
                .open_table(table(family))
                .map_err(|error| error.to_string())?;
            for (key, value) in entries {
                table
                    .insert(&key[..], &value[..])
                    .map_err(|error| error.to_string())?;
            }
        }
        txn.commit().map_err(|error| error.to_string())
    }
}
