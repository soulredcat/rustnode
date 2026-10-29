//! The mempool `GET /mempool/transactions` and the `MemPooled` status are
//! served from: transactions acquired over P2P (`TxsResponse`), removed when
//! a block holding them enters the best view of their group and returned
//! when one leaves it (`FlowUtils.updateMemPoolUnsafe`), and expired like the
//! official `cleanUnconfirmedTxs`. Transactions are not validated against
//! state here; the peers are official nodes, which relay only what they
//! accepted.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use alephium_chain::ChainStore;
use alephium_model::{Hash, TransactionTemplate};

/// `unconfirmed-tx-expiry-duration`.
pub const EXPIRY_MILLIS: u64 = 10 * 60 * 1000;
/// `clean-mempool-frequency`.
pub const CLEAN_EVERY_MILLIS: u64 = 10 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MempoolTx {
    pub chain_from: u32,
    pub chain_to: u32,
    pub template: TransactionTemplate,
    /// When this node first saw it (`seenAt`), in milliseconds.
    pub seen_at: u64,
}

/// Bounded, insertion-ordered mempool, with the best view of every chain
/// index it was last updated against.
#[derive(Debug)]
pub struct Mempool {
    capacity: usize,
    order: VecDeque<Hash>,
    txs: HashMap<Hash, MempoolTx>,
    views: HashMap<(u32, u32), Vec<Hash>>,
    last_clean: u64,
}

pub type SharedMempool = Arc<Mutex<Mempool>>;

impl Mempool {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            order: VecDeque::with_capacity(capacity),
            txs: HashMap::with_capacity(capacity),
            views: HashMap::new(),
            last_clean: 0,
        }
    }

    #[must_use]
    pub fn shared(capacity: usize) -> SharedMempool {
        Arc::new(Mutex::new(Self::new(capacity)))
    }

    /// Adds a transaction; `false` for one already held. When full, the
    /// oldest goes.
    pub fn insert(&mut self, tx: MempoolTx) -> bool {
        let id = tx.template.unsigned.id();
        if self.txs.contains_key(&id) {
            return false;
        }
        while self.txs.len() >= self.capacity {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.txs.remove(&oldest);
        }
        self.order.push_back(id);
        self.txs.insert(id, tx);
        true
    }

    fn remove(&mut self, id: &Hash) {
        if self.txs.remove(id).is_some() {
            self.order.retain(|held| held != id);
        }
    }

    /// Whether the mempool of `group` holds the transaction
    /// (`TxUtils.isInMemPool`).
    #[must_use]
    pub fn holds(&self, id: &Hash, group: u32) -> bool {
        self.txs.get(id).is_some_and(|tx| tx.chain_from == group)
    }

    /// The transactions in arrival order.
    pub fn iter(&self) -> impl Iterator<Item = &MempoolTx> {
        self.order.iter().filter_map(|id| self.txs.get(id))
    }

    /// `cleanUnconfirmedTxs`, every `CLEAN_EVERY_MILLIS`: transactions seen
    /// more than `EXPIRY_MILLIS` ago go.
    pub fn clean(&mut self, now: u64) {
        if now < self.last_clean + CLEAN_EVERY_MILLIS {
            return;
        }
        self.last_clean = now;
        let threshold = now.saturating_sub(EXPIRY_MILLIS);
        let expired: Vec<Hash> = self
            .iter()
            .filter(|tx| tx.seen_at < threshold)
            .map(|tx| tx.template.unsigned.id())
            .collect();
        for id in &expired {
            self.remove(id);
        }
    }

    /// `BlockFlow.updateViewPerChainIndexDanubeUnsafe` for the mempool: the
    /// best view of `from -> to` moved, so transactions of blocks that left
    /// it return and those of blocks that entered it go (`MemPool.reorg`
    /// adds first, then removes).
    pub fn follow_view(&mut self, store: &ChainStore, from: u32, to: u32, now: u64) {
        let Ok(new_deps) = store.best_deps(from, to) else {
            return;
        };
        if let Some(old_deps) = self.views.get(&(from, to))
            && let Ok(Some(change)) = store.view_change(from, old_deps, &new_deps)
        {
            for block in &change.left {
                let Some(entry) = store.get(block) else {
                    continue;
                };
                for tx in non_coinbase(&entry.block.transactions) {
                    self.insert(MempoolTx {
                        chain_from: entry.from,
                        chain_to: entry.to,
                        template: TransactionTemplate {
                            unsigned: tx.unsigned.clone(),
                            input_signatures: tx.input_signatures.clone(),
                            script_signatures: tx.script_signatures.clone(),
                        },
                        seen_at: now,
                    });
                }
            }
            for block in &change.entered {
                if let Some(entry) = store.get(block) {
                    for tx in non_coinbase(&entry.block.transactions) {
                        self.remove(&tx.id());
                    }
                }
            }
        }
        self.views.insert((from, to), new_deps);
    }
}

/// `Block.nonCoinbase`: every transaction but the last.
fn non_coinbase<T>(transactions: &[T]) -> &[T] {
    transactions.split_last().map_or(&[], |(_, rest)| rest)
}

#[cfg(test)]
mod tests {
    use alephium_model::{TransactionTemplate, U256, UnsignedTransaction};

    use super::{CLEAN_EVERY_MILLIS, EXPIRY_MILLIS, Mempool, MempoolTx, non_coinbase};

    fn tx(gas: i32, seen_at: u64) -> MempoolTx {
        MempoolTx {
            chain_from: 1,
            chain_to: 2,
            template: TransactionTemplate {
                unsigned: UnsignedTransaction {
                    version: 0,
                    network_id: 0,
                    script: None,
                    gas_amount: gas,
                    gas_price: U256::ONE,
                    inputs: Vec::new(),
                    fixed_outputs: Vec::new(),
                },
                input_signatures: Vec::new(),
                script_signatures: Vec::new(),
            },
            seen_at,
        }
    }

    #[test]
    fn duplicates_are_rejected_and_the_oldest_goes_when_full() {
        let mut pool = Mempool::new(2);
        assert!(pool.insert(tx(1, 0)));
        assert!(!pool.insert(tx(1, 5)));
        assert!(pool.insert(tx(2, 0)));
        assert!(pool.insert(tx(3, 0)));
        let gas: Vec<i32> = pool
            .iter()
            .map(|tx| tx.template.unsigned.gas_amount)
            .collect();
        assert_eq!(gas, vec![2, 3]);
        let id = tx(3, 0).template.unsigned.id();
        assert!(pool.holds(&id, 1));
        assert!(!pool.holds(&id, 2));
    }

    #[test]
    fn expiry_runs_every_clean_period() {
        let mut pool = Mempool::new(8);
        pool.insert(tx(1, 1_000));
        pool.insert(tx(2, EXPIRY_MILLIS + 5_000));
        pool.clean(CLEAN_EVERY_MILLIS + 2_000);
        // Seen 1 s in, more than the expiry before the clean: gone.
        assert_eq!(pool.iter().count(), 1);
        pool.insert(tx(3, 0));
        // The next clean is a whole period later.
        pool.clean(CLEAN_EVERY_MILLIS + 3_000);
        assert_eq!(pool.iter().count(), 2);
    }

    #[test]
    fn the_coinbase_is_the_last_transaction() {
        assert_eq!(non_coinbase(&[1, 2, 3]), &[1, 2]);
        assert!(non_coinbase::<u8>(&[]).is_empty());
    }
}
