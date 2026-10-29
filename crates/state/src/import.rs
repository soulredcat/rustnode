//! Copies world states out of an official node's RocksDB, opened as a
//! secondary instance (read only, beside the running node): every trie node
//! reachable from a block's roots, each checked against the hash it is
//! stored under, with the contract immutable states and code the contract
//! trie refers to.

use std::collections::HashSet;
use std::path::Path;

use alephium_model::{Hash, OutputRef, Val, deserialize};
use rocksdb::{DB, Options};

use crate::store::{Roots, StateStore, Writer};
use crate::trie::{Node, NodeSource};

/// `Storages.trieHashPostfix`: a block's world state roots are stored in
/// the `all` column family under its hash and this byte.
const TRIE_HASH_POSTFIX: u8 = 2;
/// Block cache for the official database's index, filter and data blocks.
const BLOCK_CACHE_BYTES: usize = 256 * 1024 * 1024;
/// Readers fetching trie nodes at once: random reads of 32 KiB blocks on
/// the official node's SSD are fast only when many are in flight.
const READERS: usize = 16;
/// Nodes fetched per round.
const ROUND: usize = 16 * 1024;
const PROGRESS_EVERY: usize = 1_000_000;
/// `Either` tags of `ContractStorageImmutableState`.
const IMMUTABLE_STATE: u8 = 0;
const CONTRACT_CODE: u8 = 1;

pub struct Official {
    db: DB,
}

impl Official {
    /// Opens the database at `path` as a secondary instance keeping its own
    /// files in `secondary`.
    pub fn open(path: &Path, secondary: &Path) -> Result<Self, String> {
        let at = |error: rocksdb::Error| format!("{}: {error}", path.display());
        // The database's own options: its column families' bloom filters,
        // without which every read searches each level.
        let env = rocksdb::Env::new().map_err(at)?;
        let cache = rocksdb::Cache::new_lru_cache(BLOCK_CACHE_BYTES);
        let (mut options, families) = Options::load_latest(path, env, true, cache).map_err(at)?;
        options.set_max_open_files(-1);
        let db = DB::open_cf_descriptors_as_secondary(&options, path, secondary, families)
            .map_err(at)?;
        Ok(Self { db })
    }

    /// Reads what the running node wrote since the database was opened or
    /// last caught up: a secondary instance otherwise sees that moment only.
    pub fn catch_up(&self) -> Result<(), String> {
        self.db
            .try_catch_up_with_primary()
            .map_err(|error| error.to_string())
    }

    fn get(&self, family: &str, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
        let handle = self
            .db
            .cf_handle(family)
            .ok_or_else(|| format!("no `{family}` column family"))?;
        self.db
            .get_cf(&handle, key)
            .map_err(|error| error.to_string())
    }

    /// `WorldStateStorage.get`: the roots of the world state after `block`.
    pub fn roots(&self, block: &Hash) -> Result<Option<Roots>, String> {
        let mut key = block.0.to_vec();
        key.push(TRIE_HASH_POSTFIX);
        let Some(bytes) = self.get("all", &key)? else {
            return Ok(None);
        };
        let bytes: [u8; 96] = bytes.as_slice().try_into().map_err(|_| {
            format!(
                "world state roots of {} are {} bytes",
                block.to_hex(),
                bytes.len()
            )
        })?;
        Ok(Some(Roots::from_bytes(&bytes)))
    }

    /// The `trie` entries under `hashes`, read by `READERS` threads.
    fn trie_entries(&self, hashes: &[Hash]) -> Result<Vec<Vec<u8>>, String> {
        let per_reader = hashes.len().div_ceil(READERS).max(1);
        std::thread::scope(|scope| {
            let readers: Vec<_> = hashes
                .chunks(per_reader)
                .map(|part| {
                    scope.spawn(move || {
                        part.iter()
                            .map(|hash| self.trie_entry(hash, 0))
                            .collect::<Result<Vec<_>, _>>()
                    })
                })
                .collect();
            let mut entries = Vec::with_capacity(hashes.len());
            for reader in readers {
                entries.extend(reader.join().map_err(|_| "a trie reader panicked")??);
            }
            Ok(entries)
        })
    }

    /// The `trie` entry under `hash`, whose bytes after `skip` must hash to it.
    fn trie_entry(&self, hash: &Hash, skip: usize) -> Result<Vec<u8>, String> {
        let bytes = self
            .get("trie", &hash.0)?
            .ok_or_else(|| format!("the official node has no trie entry {}", hash.to_hex()))?;
        if bytes.len() < skip || Hash::blake2b(&bytes[skip..]) != *hash {
            return Err(format!(
                "trie entry {} does not hash to its key",
                hash.to_hex()
            ));
        }
        Ok(bytes)
    }
}

/// Entries written per commit when a column family is copied.
const FAMILY_BATCH: usize = 50_000;

impl Official {
    /// Copies the event and sub-contract column families as they are, in
    /// key order; returns the entries and bytes copied per family.
    pub fn copy_families(&self, store: &StateStore) -> Result<Vec<(String, u64, u64)>, String> {
        let mut copied = Vec::new();
        for family in crate::events::FAMILIES {
            let handle = self
                .db
                .cf_handle(family)
                .ok_or_else(|| format!("no `{family}` column family"))?;
            let (mut entries, mut bytes) = (0u64, 0u64);
            let mut batch = Vec::with_capacity(FAMILY_BATCH);
            let mut page_blocks: std::collections::HashSet<Hash> = std::collections::HashSet::new();
            for item in self.db.iterator_cf(&handle, rocksdb::IteratorMode::Start) {
                let (key, value) = item.map_err(|error| error.to_string())?;
                entries += 1;
                bytes += (key.len() + value.len()) as u64;
                // An event page (a longer key than a hash) starts with its
                // block's hash.
                if family == "log" && key.len() > 32 && value.len() >= 32 {
                    let mut block = [0u8; 32];
                    block.copy_from_slice(&value[..32]);
                    page_blocks.insert(Hash(block));
                }
                batch.push((key.to_vec(), value.to_vec()));
                if batch.len() == FAMILY_BATCH {
                    store.put_raw(family, &batch)?;
                    batch.clear();
                }
                if entries.is_multiple_of(5_000_000) {
                    println!("state: {family}: {entries} entries copied");
                }
            }
            store.put_raw(family, &batch)?;
            copied.push((family.to_owned(), entries, bytes));
            if !page_blocks.is_empty() {
                let blocks: Vec<Hash> = page_blocks.into_iter().collect();
                self.copy_block_times(store, &blocks)?;
            }
        }
        store.flush_durable()?;
        Ok(copied)
    }
}

impl Official {
    /// The timestamps of `blocks`, read from their headers by `READERS`
    /// threads.
    fn copy_block_times(&self, store: &StateStore, blocks: &[Hash]) -> Result<(), String> {
        let handle = self
            .db
            .cf_handle("header")
            .ok_or("no `header` column family")?;
        for (round, chunk) in blocks.chunks(ROUND).enumerate() {
            let per_reader = chunk.len().div_ceil(READERS).max(1);
            let times = std::thread::scope(|scope| {
                let readers: Vec<_> = chunk
                    .chunks(per_reader)
                    .map(|part| {
                        let handle = &handle;
                        scope.spawn(move || {
                            part.iter()
                                .map(|block| {
                                    let header = self
                                        .db
                                        .get_cf(handle, block.0)
                                        .map_err(|error| error.to_string())?
                                        .ok_or_else(|| format!("no header {}", block.to_hex()))?;
                                    let header =
                                        deserialize::<alephium_model::BlockHeader>(&header)
                                            .map_err(|error| error.to_string())?;
                                    Ok((*block, header.timestamp.0))
                                })
                                .collect::<Result<Vec<_>, String>>()
                        })
                    })
                    .collect();
                let mut times = Vec::with_capacity(chunk.len());
                for reader in readers {
                    times.extend(reader.join().map_err(|_| "a header reader panicked")??);
                }
                Ok::<_, String>(times)
            })?;
            store.put_block_times(&times)?;
            if (round + 1) % 64 == 0 {
                println!("state: {} block times copied", (round + 1) * ROUND);
            }
        }
        Ok(())
    }
}

/// The official node's trie nodes, unchecked.
impl NodeSource for Official {
    fn node(&self, hash: &Hash) -> Result<Option<Vec<u8>>, String> {
        self.get("trie", &hash.0)
    }
}

/// What a copy wrote.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Copied {
    pub nodes: usize,
    pub bytes: usize,
    pub contracts: usize,
}

/// Copies the world state after `block` into `store`, which keeps its
/// roots once every node is in. Entries already stored are not read again.
pub fn copy_state(
    official: &Official,
    store: &StateStore,
    block: &Hash,
) -> Result<(Roots, Copied), String> {
    let roots = official.roots(block)?.ok_or_else(|| {
        format!(
            "the official node has no world state for {}",
            block.to_hex()
        )
    })?;
    let mut writer = store.writer()?;
    let mut copied = Copied::default();
    for (root, contracts) in [
        (roots.outputs, false),
        (roots.contracts, true),
        (roots.code, false),
    ] {
        copy_trie(official, store, &mut writer, root, contracts, &mut copied)?;
    }
    writer.finish(block, &roots)?;
    Ok((roots, copied))
}

/// Breadth first, a round of nodes at a time. Nodes already stored are
/// skipped with their subtrees: the store holds whole subtrees only, as an
/// interrupted import starts over.
fn copy_trie(
    official: &Official,
    store: &StateStore,
    writer: &mut Writer,
    root: Hash,
    contracts: bool,
    copied: &mut Copied,
) -> Result<(), String> {
    let mut frontier = vec![root];
    while !frontier.is_empty() {
        let mut next = Vec::new();
        let mut queued = HashSet::new();
        for round in frontier.chunks(ROUND) {
            let mut wanted = Vec::with_capacity(round.len());
            for hash in round {
                if !writer.has(hash)? {
                    wanted.push(*hash);
                }
            }
            for (hash, bytes) in wanted.iter().zip(official.trie_entries(&wanted)?) {
                let node = Node::decode(&bytes)
                    .map_err(|error| format!("trie node {}: {error}", hash.to_hex()))?;
                match node {
                    Node::Branch { children, .. } => {
                        next.extend(
                            children
                                .into_iter()
                                .flatten()
                                .filter(|child| queued.insert(*child)),
                        );
                    }
                    Node::Leaf { data, .. } if contracts => {
                        copy_contract(official, store, writer, &data, copied)?;
                    }
                    Node::Leaf { .. } => {}
                }
                copied.nodes += 1;
                copied.bytes += bytes.len();
                writer.put(store, hash, &bytes)?;
                if copied.nodes.is_multiple_of(PROGRESS_EVERY) {
                    println!("state: {} trie nodes copied", copied.nodes);
                }
            }
        }
        frontier = next;
    }
    Ok(())
}

/// `ContractStorageState`: a `ContractMutableState` (since Leman) refers to
/// its immutable state and, through it, its code, both kept beside the trie
/// nodes; a `ContractLegacyState` has its code in the code trie.
fn copy_contract(
    official: &Official,
    store: &StateStore,
    writer: &mut Writer,
    state: &[u8],
    copied: &mut Copied,
) -> Result<(), String> {
    copied.contracts += 1;
    let Ok((_, (_, immutable_hash))) = deserialize::<(Vec<Val>, (OutputRef, Hash))>(state) else {
        return Ok(());
    };
    if writer.has(&immutable_hash)? {
        return Ok(());
    }
    let immutable = official.trie_entry(&immutable_hash, 1)?;
    if immutable[0] != IMMUTABLE_STATE {
        return Err(format!(
            "contract immutable state {} is not one",
            immutable_hash.to_hex()
        ));
    }
    // `ContractImmutableState`: code hash, initial state hash, fields.
    let (code_hash, _) =
        deserialize::<(Hash, (Hash, Vec<Val>))>(&immutable[1..]).map_err(|error| {
            format!(
                "contract immutable state {}: {error}",
                immutable_hash.to_hex()
            )
        })?;
    if !writer.has(&code_hash)? {
        let code = official.trie_entry(&code_hash, 1)?;
        if code[0] != CONTRACT_CODE {
            return Err(format!("contract code {} is not code", code_hash.to_hex()));
        }
        writer.put(store, &code_hash, &code)?;
    }
    writer.put(store, &immutable_hash, &immutable)
}
