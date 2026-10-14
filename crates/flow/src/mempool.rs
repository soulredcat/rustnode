use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// A transaction exactly as received in a P2P `TxsResponse`. Nothing here is
/// validated: the bytes are an unverified `TransactionTemplate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawMempoolTx {
    pub chain_from: i32,
    pub chain_to: i32,
    pub raw: Vec<u8>,
    pub seen_at_unix_ms: i64,
}

/// Bounded, insertion-ordered store of raw transactions acquired from P2P.
///
/// This is the data `GET /mempool/transactions` will be served from. When full,
/// the oldest transaction is evicted.
#[derive(Debug)]
pub struct RawMempool {
    capacity: usize,
    order: VecDeque<[u8; 32]>,
    txs: HashMap<[u8; 32], RawMempoolTx>,
}

pub type SharedRawMempool = Arc<Mutex<RawMempool>>;

impl RawMempool {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            order: VecDeque::with_capacity(capacity),
            txs: HashMap::with_capacity(capacity),
        }
    }

    #[must_use]
    pub fn shared(capacity: usize) -> SharedRawMempool {
        Arc::new(Mutex::new(Self::new(capacity)))
    }

    /// Inserts a transaction. Returns `false` for an ID already present.
    pub fn insert(&mut self, transaction_id: [u8; 32], tx: RawMempoolTx) -> bool {
        if self.txs.contains_key(&transaction_id) {
            return false;
        }
        while self.txs.len() >= self.capacity {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.txs.remove(&oldest);
        }
        self.order.push_back(transaction_id);
        self.txs.insert(transaction_id, tx);
        true
    }

    #[must_use]
    pub fn get(&self, transaction_id: &[u8; 32]) -> Option<&RawMempoolTx> {
        self.txs.get(transaction_id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.txs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.txs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{RawMempool, RawMempoolTx};

    fn tx(byte: u8) -> RawMempoolTx {
        RawMempoolTx {
            chain_from: 0,
            chain_to: 1,
            raw: vec![byte],
            seen_at_unix_ms: i64::from(byte),
        }
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let mut pool = RawMempool::new(4);
        assert!(pool.insert([1; 32], tx(1)));
        assert!(!pool.insert([1; 32], tx(2)));
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.get(&[1; 32]), Some(&tx(1)));
    }

    #[test]
    fn full_pool_evicts_oldest_first() {
        let mut pool = RawMempool::new(2);
        assert!(pool.insert([1; 32], tx(1)));
        assert!(pool.insert([2; 32], tx(2)));
        assert!(pool.insert([3; 32], tx(3)));
        assert_eq!(pool.len(), 2);
        assert_eq!(pool.get(&[1; 32]), None);
        assert_eq!(pool.get(&[2; 32]), Some(&tx(2)));
        assert_eq!(pool.get(&[3; 32]), Some(&tx(3)));
    }

    #[test]
    fn zero_capacity_still_holds_one_transaction() {
        let mut pool = RawMempool::new(0);
        assert!(pool.insert([1; 32], tx(1)));
        assert!(pool.insert([2; 32], tx(2)));
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.get(&[2; 32]), Some(&tx(2)));
    }
}
