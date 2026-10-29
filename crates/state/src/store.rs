//! The world state on disk, in its own redb file: trie nodes and contract
//! immutable states under their hashes (the official `trie` column family),
//! and the world state roots of each block (`WorldState.Hashes`).
//!
//! An import writes nodes before their subtrees are complete, so it is
//! marked on disk while it runs; a store opened with the mark set was
//! interrupted mid-import and starts empty again.

use std::path::Path;

use alephium_model::Hash;
use redb::{Database, Durability, ReadableDatabase, ReadableTable, TableDefinition};

const NODES: TableDefinition<[u8; 32], &[u8]> = TableDefinition::new("nodes");
const ROOTS: TableDefinition<[u8; 32], [u8; 96]> = TableDefinition::new("roots");
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
const IMPORTING: &str = "importing";
/// redb's page cache; the rest is read through the OS page cache.
const CACHE_BYTES: usize = 64 * 1024 * 1024;

/// `WorldState.Hashes`: the roots of the output, contract and code tries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Roots {
    pub outputs: Hash,
    pub contracts: Hash,
    pub code: Hash,
}

impl Roots {
    #[must_use]
    pub fn from_bytes(bytes: &[u8; 96]) -> Self {
        let hash = |at: usize| {
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&bytes[at..at + 32]);
            Hash(hash)
        };
        Self {
            outputs: hash(0),
            contracts: hash(32),
            code: hash(64),
        }
    }

    #[must_use]
    pub fn to_bytes(&self) -> [u8; 96] {
        let mut bytes = [0u8; 96];
        bytes[..32].copy_from_slice(&self.outputs.0);
        bytes[32..64].copy_from_slice(&self.contracts.0);
        bytes[64..].copy_from_slice(&self.code.0);
        bytes
    }

    /// `Hashes.stateHash`, which a block's `depStateHash` commits to: the
    /// code trie is not part of it.
    #[must_use]
    pub fn state_hash(&self) -> Hash {
        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(&self.outputs.0);
        bytes[32..].copy_from_slice(&self.contracts.0);
        Hash::blake2b(&bytes)
    }
}

pub struct StateStore {
    db: Database,
}

impl crate::trie::NodeSource for StateStore {
    fn node(&self, hash: &Hash) -> Result<Option<Vec<u8>>, String> {
        StateStore::node(self, hash)
    }
}

impl StateStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let db = Database::builder()
            .set_cache_size(CACHE_BYTES)
            .create(path)
            .map_err(|error| error.to_string())?;
        let txn = db.begin_write().map_err(|error| error.to_string())?;
        let interrupted = txn
            .open_table(META)
            .map_err(|error| error.to_string())?
            .get(IMPORTING)
            .map_err(|error| error.to_string())?
            .is_some();
        if interrupted {
            eprintln!(
                "state: {} holds an interrupted import; starting again",
                path.display()
            );
            txn.delete_table(NODES).map_err(|error| error.to_string())?;
            txn.delete_table(ROOTS).map_err(|error| error.to_string())?;
            txn.delete_table(META).map_err(|error| error.to_string())?;
        }
        txn.open_table(NODES).map_err(|error| error.to_string())?;
        txn.open_table(ROOTS).map_err(|error| error.to_string())?;
        txn.open_table(META).map_err(|error| error.to_string())?;
        txn.commit().map_err(|error| error.to_string())?;
        Ok(Self { db })
    }

    pub fn node(&self, hash: &Hash) -> Result<Option<Vec<u8>>, String> {
        let txn = self.db.begin_read().map_err(|error| error.to_string())?;
        let table = txn.open_table(NODES).map_err(|error| error.to_string())?;
        let value = table.get(hash.0).map_err(|error| error.to_string())?;
        Ok(value.map(|value| value.value().to_vec()))
    }

    pub fn roots(&self, block: &Hash) -> Result<Option<Roots>, String> {
        let txn = self.db.begin_read().map_err(|error| error.to_string())?;
        let table = txn.open_table(ROOTS).map_err(|error| error.to_string())?;
        let value = table.get(block.0).map_err(|error| error.to_string())?;
        Ok(value.map(|value| Roots::from_bytes(&value.value())))
    }

    /// Keeps the world state after `block`: its new trie nodes and
    /// immutable entries, then its roots, in one commit.
    pub fn put_state<'n>(
        &self,
        block: &Hash,
        roots: &Roots,
        nodes: impl Iterator<Item = (&'n Hash, &'n Vec<u8>)>,
    ) -> Result<(), String> {
        let txn = self.db.begin_write().map_err(|error| error.to_string())?;
        {
            let mut table = txn.open_table(NODES).map_err(|error| error.to_string())?;
            for (hash, bytes) in nodes {
                table
                    .insert(hash.0, bytes.as_slice())
                    .map_err(|error| error.to_string())?;
            }
            txn.open_table(ROOTS)
                .map_err(|error| error.to_string())?
                .insert(block.0, roots.to_bytes())
                .map_err(|error| error.to_string())?;
        }
        txn.commit().map_err(|error| error.to_string())
    }

    /// Starts an import: the mark goes to disk first.
    pub(crate) fn writer(&self) -> Result<Writer, String> {
        let txn = self.db.begin_write().map_err(|error| error.to_string())?;
        txn.open_table(META)
            .map_err(|error| error.to_string())?
            .insert(IMPORTING, &[][..])
            .map_err(|error| error.to_string())?;
        txn.commit().map_err(|error| error.to_string())?;
        self.continue_writing()
    }

    fn continue_writing(&self) -> Result<Writer, String> {
        let mut txn = self.db.begin_write().map_err(|error| error.to_string())?;
        txn.set_durability(Durability::None)
            .map_err(|error| error.to_string())?;
        Ok(Writer {
            txn: Some(txn),
            written: 0,
        })
    }
}

/// Node writes, committed every `COMMIT_EVERY` nodes without fsync; the
/// roots are written last, durably, with the import mark removed.
pub(crate) struct Writer {
    /// Always `Some` between calls; taken only to commit.
    txn: Option<redb::WriteTransaction>,
    written: usize,
}

const COMMIT_EVERY: usize = 200_000;

impl Writer {
    fn txn(&self) -> Result<&redb::WriteTransaction, String> {
        self.txn.as_ref().ok_or_else(|| "no open write".to_owned())
    }

    pub(crate) fn has(&self, hash: &Hash) -> Result<bool, String> {
        let table = self
            .txn()?
            .open_table(NODES)
            .map_err(|error| error.to_string())?;
        Ok(table
            .get(hash.0)
            .map_err(|error| error.to_string())?
            .is_some())
    }

    /// redb has one write at a time: the open one commits before the next
    /// begins.
    pub(crate) fn put(
        &mut self,
        store: &StateStore,
        hash: &Hash,
        bytes: &[u8],
    ) -> Result<(), String> {
        self.txn()?
            .open_table(NODES)
            .map_err(|error| error.to_string())?
            .insert(hash.0, bytes)
            .map_err(|error| error.to_string())?;
        self.written += 1;
        if self.written.is_multiple_of(COMMIT_EVERY) {
            if let Some(done) = self.txn.take() {
                done.commit().map_err(|error| error.to_string())?;
            }
            self.txn = store.continue_writing()?.txn;
        }
        Ok(())
    }

    pub(crate) fn finish(mut self, block: &Hash, roots: &Roots) -> Result<(), String> {
        let mut txn = self.txn.take().ok_or("no open write")?;
        txn.set_durability(Durability::Immediate)
            .map_err(|error| error.to_string())?;
        txn.open_table(ROOTS)
            .map_err(|error| error.to_string())?
            .insert(block.0, roots.to_bytes())
            .map_err(|error| error.to_string())?;
        txn.open_table(META)
            .map_err(|error| error.to_string())?
            .remove(IMPORTING)
            .map_err(|error| error.to_string())?;
        txn.commit().map_err(|error| error.to_string())
    }
}
