#![forbid(unsafe_code)]
//! On-disk storage of the kept block window, in one redb file: every block
//! of the window with its height and weight, the hashes at each height of
//! each chain (canonical first), the blocks holding each transaction, and
//! what the store was built for.
//!
//! Writes are collected in memory and committed together by
//! [`WindowStore::flush`]; reads see them before the commit. A commit is
//! fsynced when the last fsynced one is older than [`DURABLE_INTERVAL`], and
//! when the file is closed cleanly. After a crash the file reopens at the
//! last fsynced commit, which is always a consistent store.
//!
//! Values are raw bytes; `alephium-chain` decides what they mean.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::ops::RangeInclusive;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use redb::{Database, Durability, ReadableDatabase, TableDefinition};

pub type Hash = [u8; 32];

const BLOCKS: TableDefinition<Hash, &[u8]> = TableDefinition::new("blocks");
/// `(chain, height)` to the hashes at that height, canonical first.
const HEIGHTS: TableDefinition<(u32, u32), &[u8]> = TableDefinition::new("heights");
/// Transaction id to the blocks of the window holding it: `(block hash,
/// index in the block)` pairs, 36 bytes each, in the order blocks arrived.
const TXS: TableDefinition<Hash, &[u8]> = TableDefinition::new("txs");
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
const META_KEY: &str = "meta";
/// 2: the transaction index. A store of another version is rebuilt.
const FORMAT_VERSION: u32 = 2;

/// Longest time between fsynced commits: what a crash can lose.
pub const DURABLE_INTERVAL: Duration = Duration::from_secs(10);
/// redb's page cache (1 GiB by default); older blocks are read through the
/// OS page cache instead.
const CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "chain store: {}", self.0)
    }
}

impl std::error::Error for Error {}

macro_rules! from_redb_errors {
    ($($error:ty),*) => {$(
        impl From<$error> for Error {
            fn from(error: $error) -> Self {
                Self(error.to_string())
            }
        }
    )*};
}

from_redb_errors!(
    redb::DatabaseError,
    redb::TransactionError,
    redb::TableError,
    redb::StorageError,
    redb::CommitError,
    redb::SetDurabilityError
);

/// What a store holds: its group count, the genesis hash of every chain
/// (`from * groups + to`), and whether every height it should hold is in it
/// (false while a bootstrap is still importing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub groups: u32,
    pub genesis: Vec<Hash>,
    pub complete: bool,
}

impl Meta {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(9 + 32 * self.genesis.len());
        out.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
        out.extend_from_slice(&self.groups.to_be_bytes());
        out.push(u8::from(self.complete));
        out.extend(self.genesis.iter().flatten());
        out
    }

    fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < 9 {
            return Err(Error("short meta record".into()));
        }
        let version = be_u32(&bytes[0..4]);
        if version != FORMAT_VERSION {
            return Err(Error(format!(
                "format version {version}, this build reads {FORMAT_VERSION}"
            )));
        }
        Ok(Self {
            groups: be_u32(&bytes[4..8]),
            complete: bytes[8] != 0,
            genesis: decode_hashes(&bytes[9..])?,
        })
    }
}

/// A block of the window: its height, its weight when known (big-endian),
/// and its official serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub height: u32,
    pub weight: Option<[u8; 32]>,
    pub block: Vec<u8>,
}

impl Record {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(37 + self.block.len());
        out.extend_from_slice(&self.height.to_be_bytes());
        match &self.weight {
            Some(weight) => {
                out.push(1);
                out.extend_from_slice(weight);
            }
            None => out.push(0),
        }
        out.extend_from_slice(&self.block);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let short = || Error("short block record".into());
        if bytes.len() < 5 {
            return Err(short());
        }
        let height = be_u32(&bytes[0..4]);
        let (weight, block) = match bytes[4] {
            0 => (None, &bytes[5..]),
            1 => {
                let weight = bytes.get(5..37).ok_or_else(short)?;
                let mut out = [0u8; 32];
                out.copy_from_slice(weight);
                (Some(out), &bytes[37..])
            }
            flag => return Err(Error(format!("block record weight flag {flag}"))),
        };
        Ok(Self {
            height,
            weight,
            block: block.to_vec(),
        })
    }
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn decode_locations(bytes: &[u8]) -> Result<Vec<(Hash, u32)>, Error> {
    let (locations, rest) = bytes.as_chunks::<36>();
    if !rest.is_empty() {
        return Err(Error(format!(
            "{} bytes is not a list of transaction locations",
            bytes.len()
        )));
    }
    Ok(locations
        .iter()
        .map(|location| {
            let mut block = [0u8; 32];
            block.copy_from_slice(&location[..32]);
            (block, be_u32(&location[32..]))
        })
        .collect())
}

fn encode_locations(locations: &[(Hash, u32)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(36 * locations.len());
    for (block, index) in locations {
        out.extend_from_slice(block);
        out.extend_from_slice(&index.to_be_bytes());
    }
    out
}

fn decode_hashes(bytes: &[u8]) -> Result<Vec<Hash>, Error> {
    let (hashes, rest) = bytes.as_chunks::<32>();
    if !rest.is_empty() {
        return Err(Error(format!(
            "{} bytes is not a list of hashes",
            bytes.len()
        )));
    }
    Ok(hashes.to_vec())
}

/// Writes waiting for a commit; `None` deletes.
#[derive(Default)]
struct Batch {
    blocks: HashMap<Hash, Option<Vec<u8>>>,
    heights: BTreeMap<(u32, u32), Option<Vec<u8>>>,
    txs: HashMap<Hash, Option<Vec<u8>>>,
    meta: Option<Vec<u8>>,
    bytes: usize,
}

impl Batch {
    fn is_empty(&self) -> bool {
        self.blocks.is_empty()
            && self.heights.is_empty()
            && self.txs.is_empty()
            && self.meta.is_none()
    }
}

/// An open store file. Clones share it; one [`WindowStore`] writes to it at a
/// time.
#[derive(Clone)]
pub struct Disk {
    db: Arc<Database>,
}

impl Disk {
    /// Opens the file, creating it and its directory when missing.
    pub fn open(path: &Path) -> Result<Self, Error> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| Error(format!("{}: {error}", parent.display())))?;
        }
        let db = Database::builder()
            .set_cache_size(CACHE_BYTES)
            .create(path)?;
        // Every table exists from the start, so reads never meet a missing one.
        let txn = db.begin_write()?;
        txn.open_table(BLOCKS)?;
        txn.open_table(HEIGHTS)?;
        txn.open_table(TXS)?;
        txn.open_table(META)?;
        txn.commit()?;
        Ok(Self { db: Arc::new(db) })
    }
}

/// The kept window on disk, with the writes not committed yet.
pub struct WindowStore {
    disk: Disk,
    pending: Batch,
    last_durable: Instant,
    /// A commit made since the last fsynced one.
    unsynced: bool,
}

impl WindowStore {
    #[must_use]
    pub fn new(disk: Disk) -> Self {
        Self {
            disk,
            pending: Batch::default(),
            last_durable: Instant::now(),
            unsynced: false,
        }
    }

    pub fn meta(&self) -> Result<Option<Meta>, Error> {
        if let Some(bytes) = &self.pending.meta {
            return Meta::decode(bytes).map(Some);
        }
        let txn = self.disk.db.begin_read()?;
        let table = txn.open_table(META)?;
        let value = table.get(META_KEY)?;
        value.map(|value| Meta::decode(value.value())).transpose()
    }

    pub fn put_meta(&mut self, meta: &Meta) {
        let bytes = meta.encode();
        self.pending.bytes += bytes.len();
        self.pending.meta = Some(bytes);
    }

    pub fn record(&self, hash: &Hash) -> Result<Option<Record>, Error> {
        if let Some(pending) = self.pending.blocks.get(hash) {
            return pending.as_deref().map(Record::decode).transpose();
        }
        let txn = self.disk.db.begin_read()?;
        let table = txn.open_table(BLOCKS)?;
        let value = table.get(hash)?;
        value.map(|value| Record::decode(value.value())).transpose()
    }

    pub fn contains(&self, hash: &Hash) -> Result<bool, Error> {
        if let Some(pending) = self.pending.blocks.get(hash) {
            return Ok(pending.is_some());
        }
        let txn = self.disk.db.begin_read()?;
        let table = txn.open_table(BLOCKS)?;
        let value = table.get(hash)?;
        Ok(value.is_some())
    }

    pub fn put_record(&mut self, hash: Hash, record: &Record) {
        let bytes = record.encode();
        self.pending.bytes += bytes.len();
        self.pending.blocks.insert(hash, Some(bytes));
    }

    pub fn delete_record(&mut self, hash: Hash) {
        self.pending.blocks.insert(hash, None);
    }

    /// The hashes at a height, canonical first; empty when none are stored.
    pub fn hashes_at(&self, chain: u32, height: u32) -> Result<Vec<Hash>, Error> {
        if let Some(pending) = self.pending.heights.get(&(chain, height)) {
            return pending.as_deref().map_or(Ok(Vec::new()), decode_hashes);
        }
        let txn = self.disk.db.begin_read()?;
        let table = txn.open_table(HEIGHTS)?;
        let value = table.get((chain, height))?;
        value.map_or(Ok(Vec::new()), |value| decode_hashes(value.value()))
    }

    pub fn put_hashes(&mut self, chain: u32, height: u32, hashes: &[Hash]) {
        let bytes = hashes.concat();
        self.pending.bytes += bytes.len();
        self.pending.heights.insert((chain, height), Some(bytes));
    }

    pub fn delete_hashes(&mut self, chain: u32, height: u32) {
        self.pending.heights.insert((chain, height), None);
    }

    /// The blocks of the window holding a transaction, as `(block hash, index
    /// in the block)` in the order the blocks arrived.
    pub fn tx_locations(&self, tx: &Hash) -> Result<Vec<(Hash, u32)>, Error> {
        if let Some(pending) = self.pending.txs.get(tx) {
            return pending.as_deref().map_or(Ok(Vec::new()), decode_locations);
        }
        let txn = self.disk.db.begin_read()?;
        let table = txn.open_table(TXS)?;
        let value = table.get(tx)?;
        value.map_or(Ok(Vec::new()), |value| decode_locations(value.value()))
    }

    pub fn add_tx_location(&mut self, tx: Hash, block: Hash, index: u32) -> Result<(), Error> {
        let mut locations = self.tx_locations(&tx)?;
        if !locations.contains(&(block, index)) {
            locations.push((block, index));
        }
        let bytes = encode_locations(&locations);
        self.pending.bytes += bytes.len();
        self.pending.txs.insert(tx, Some(bytes));
        Ok(())
    }

    pub fn remove_tx_location(&mut self, tx: Hash, block: Hash) -> Result<(), Error> {
        let mut locations = self.tx_locations(&tx)?;
        locations.retain(|(holder, _)| *holder != block);
        let value = (!locations.is_empty()).then(|| encode_locations(&locations));
        self.pending.txs.insert(tx, value);
        Ok(())
    }

    /// Every stored height of `chain` in `range`, with its hashes.
    pub fn heights(
        &self,
        chain: u32,
        range: RangeInclusive<u32>,
    ) -> Result<BTreeMap<u32, Vec<Hash>>, Error> {
        let mut rows = BTreeMap::new();
        if range.is_empty() {
            return Ok(rows);
        }
        let keys = (chain, *range.start())..=(chain, *range.end());
        {
            let txn = self.disk.db.begin_read()?;
            let table = txn.open_table(HEIGHTS)?;
            for row in table.range(keys.clone())? {
                let (key, value) = row?;
                rows.insert(key.value().1, decode_hashes(value.value())?);
            }
        }
        for (&(_, height), value) in self.pending.heights.range(keys) {
            match value {
                Some(bytes) => {
                    rows.insert(height, decode_hashes(bytes)?);
                }
                None => {
                    rows.remove(&height);
                }
            }
        }
        Ok(rows)
    }

    /// The highest stored height of `chain`.
    pub fn highest_height(&self, chain: u32) -> Result<Option<u32>, Error> {
        let keys = (chain, 0)..=(chain, u32::MAX);
        let mut highest = None;
        {
            let txn = self.disk.db.begin_read()?;
            let table = txn.open_table(HEIGHTS)?;
            for row in table.range(keys.clone())?.rev() {
                let height = row?.0.value().1;
                if !matches!(self.pending.heights.get(&(chain, height)), Some(None)) {
                    highest = Some(height);
                    break;
                }
            }
        }
        for (&(_, height), value) in self.pending.heights.range(keys) {
            if value.is_some() {
                highest = highest.max(Some(height));
            }
        }
        Ok(highest)
    }

    /// Bytes waiting for a commit.
    #[must_use]
    pub fn pending_bytes(&self) -> usize {
        self.pending.bytes
    }

    /// Commits the pending writes, fsynced when the last fsynced commit is
    /// older than [`DURABLE_INTERVAL`]. On failure they stay pending.
    pub fn flush(&mut self) -> Result<(), Error> {
        let durable = self.last_durable.elapsed() >= DURABLE_INTERVAL;
        self.commit(durable)
    }

    /// Commits the pending writes and fsyncs.
    pub fn flush_durable(&mut self) -> Result<(), Error> {
        self.commit(true)
    }

    fn commit(&mut self, durable: bool) -> Result<(), Error> {
        if self.pending.is_empty() && !(durable && self.unsynced) {
            return Ok(());
        }
        let mut txn = self.disk.db.begin_write()?;
        txn.set_durability(if durable {
            Durability::Immediate
        } else {
            Durability::None
        })?;
        {
            let mut blocks = txn.open_table(BLOCKS)?;
            for (hash, value) in &self.pending.blocks {
                match value {
                    Some(bytes) => {
                        blocks.insert(hash, bytes.as_slice())?;
                    }
                    None => {
                        blocks.remove(hash)?;
                    }
                }
            }
            let mut heights = txn.open_table(HEIGHTS)?;
            for (key, value) in &self.pending.heights {
                match value {
                    Some(bytes) => {
                        heights.insert(key, bytes.as_slice())?;
                    }
                    None => {
                        heights.remove(key)?;
                    }
                }
            }
            let mut txs = txn.open_table(TXS)?;
            for (tx, value) in &self.pending.txs {
                match value {
                    Some(bytes) => {
                        txs.insert(tx, bytes.as_slice())?;
                    }
                    None => {
                        txs.remove(tx)?;
                    }
                }
            }
            if let Some(bytes) = &self.pending.meta {
                txn.open_table(META)?.insert(META_KEY, bytes.as_slice())?;
            }
        }
        txn.commit()?;
        self.pending = Batch::default();
        if durable {
            self.last_durable = Instant::now();
            self.unsynced = false;
        } else {
            self.unsynced = true;
        }
        Ok(())
    }

    /// Deletes everything, durably.
    pub fn clear(&mut self) -> Result<(), Error> {
        self.pending = Batch::default();
        let txn = self.disk.db.begin_write()?;
        txn.delete_table(BLOCKS)?;
        txn.delete_table(HEIGHTS)?;
        txn.delete_table(TXS)?;
        txn.delete_table(META)?;
        txn.open_table(BLOCKS)?;
        txn.open_table(HEIGHTS)?;
        txn.open_table(TXS)?;
        txn.open_table(META)?;
        txn.commit()?;
        self.last_durable = Instant::now();
        self.unsynced = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Disk, Meta, Record, WindowStore};

    fn temp_path(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("alephium-store-{}-{name}.redb", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn record(height: u32, weight: Option<u8>) -> Record {
        Record {
            height,
            weight: weight.map(|byte| [byte; 32]),
            block: vec![height as u8; 40],
        }
    }

    #[test]
    fn records_round_trip() {
        for value in [record(7, None), record(u32::MAX, Some(9))] {
            assert_eq!(Record::decode(&value.encode()), Ok(value));
        }
        assert!(Record::decode(&[0, 0, 0, 1]).is_err());
        assert!(Record::decode(&[0, 0, 0, 1, 1, 2]).is_err());
        assert!(Record::decode(&[0, 0, 0, 1, 2]).is_err());
    }

    #[test]
    fn reads_see_pending_writes_and_survive_reopening() {
        let path = temp_path("pending");
        let meta = Meta {
            groups: 4,
            genesis: vec![[1; 32], [2; 32]],
            complete: true,
        };
        {
            let mut store = WindowStore::new(Disk::open(&path).expect("open"));
            assert_eq!(store.meta(), Ok(None));
            store.put_meta(&meta);
            store.put_record([5; 32], &record(3, Some(1)));
            store.put_hashes(1, 3, &[[5; 32], [6; 32]]);
            store.put_hashes(1, 4, &[[7; 32]]);
            store.put_hashes(2, 9, &[[8; 32]]);
            // Visible before the commit.
            assert_eq!(store.meta(), Ok(Some(meta.clone())));
            assert_eq!(store.record(&[5; 32]), Ok(Some(record(3, Some(1)))));
            assert_eq!(store.contains(&[6; 32]), Ok(false));
            assert_eq!(store.hashes_at(1, 3), Ok(vec![[5; 32], [6; 32]]));
            assert_eq!(store.highest_height(1), Ok(Some(4)));
            store.flush().expect("flush");
            assert_eq!(store.pending_bytes(), 0);

            // Deletions hide committed rows until they are committed too.
            store.delete_hashes(1, 4);
            store.delete_record([5; 32]);
            assert_eq!(store.highest_height(1), Ok(Some(3)));
            assert_eq!(store.contains(&[5; 32]), Ok(false));
            assert_eq!(store.heights(1, 0..=u32::MAX).map(|rows| rows.len()), Ok(1));
            // A reversed range is empty, not an error.
            let (low, high) = (5, 4);
            assert_eq!(store.heights(1, low..=high).map(|rows| rows.len()), Ok(0));
            store.flush().expect("flush");
        }
        // A clean close keeps every commit, fsynced or not.
        let mut store = WindowStore::new(Disk::open(&path).expect("reopen"));
        assert_eq!(store.meta(), Ok(Some(meta)));
        assert_eq!(store.record(&[5; 32]), Ok(None));
        assert_eq!(store.hashes_at(1, 3), Ok(vec![[5; 32], [6; 32]]));
        assert_eq!(store.hashes_at(1, 4), Ok(Vec::new()));
        assert_eq!(store.highest_height(2), Ok(Some(9)));
        assert_eq!(store.highest_height(3), Ok(None));

        // Transaction locations keep arrival order and survive a commit.
        store.add_tx_location([9; 32], [5; 32], 0).expect("add");
        store.add_tx_location([9; 32], [6; 32], 3).expect("add");
        store
            .add_tx_location([9; 32], [5; 32], 0)
            .expect("add again");
        store.flush().expect("flush");
        assert_eq!(
            store.tx_locations(&[9; 32]),
            Ok(vec![([5; 32], 0), ([6; 32], 3)])
        );
        store.remove_tx_location([9; 32], [5; 32]).expect("remove");
        assert_eq!(store.tx_locations(&[9; 32]), Ok(vec![([6; 32], 3)]));
        store.remove_tx_location([9; 32], [6; 32]).expect("remove");
        store.flush().expect("flush");
        assert_eq!(store.tx_locations(&[9; 32]), Ok(Vec::new()));

        store.clear().expect("clear");
        assert_eq!(store.meta(), Ok(None));
        assert_eq!(store.highest_height(1), Ok(None));
        drop(store);
        let _ = std::fs::remove_file(&path);
    }
}
