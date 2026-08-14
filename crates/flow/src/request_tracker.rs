use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use alephium_crypto::hash_blake2b_256;
use alephium_protocol::message::payload::{ChainIndex, RequestId, TransactionId, TxsRequest};

#[derive(Debug, Clone)]
struct TrackedTransaction {
    chain_index: ChainIndex,
    transaction_id: TransactionId,
    retries: u8,
}

#[derive(Debug)]
struct InflightRequest {
    sent_at: Instant,
    transactions: Vec<TrackedTransaction>,
}

#[derive(Debug)]
pub(crate) struct CompletedTransaction {
    pub(crate) chain_index: Option<ChainIndex>,
    pub(crate) transaction_id: [u8; 32],
    pub(crate) raw_transaction: Vec<u8>,
}

#[derive(Debug, Default)]
pub(crate) struct Completion {
    pub(crate) transactions: Vec<CompletedTransaction>,
    pub(crate) requeued: usize,
    pub(crate) exhausted: usize,
    pub(crate) unknown_request: bool,
}

#[derive(Debug)]
pub(crate) struct TxRequestTracker {
    pending: VecDeque<TrackedTransaction>,
    active_ids: HashSet<[u8; 32]>,
    received_ids: HashSet<[u8; 32]>,
    received_order: VecDeque<[u8; 32]>,
    inflight: HashMap<u32, InflightRequest>,
    max_inflight: usize,
    batch_size: usize,
    max_tracked_ids: usize,
    max_received_ids: usize,
    request_timeout: Duration,
    max_retries: u8,
}

impl TxRequestTracker {
    pub(crate) fn new(
        max_inflight: usize,
        batch_size: usize,
        max_tracked_ids: usize,
        request_timeout: Duration,
        max_retries: u8,
    ) -> Self {
        let max_tracked_ids = max_tracked_ids.max(1);
        Self {
            pending: VecDeque::new(),
            active_ids: HashSet::new(),
            received_ids: HashSet::new(),
            received_order: VecDeque::new(),
            inflight: HashMap::new(),
            max_inflight: max_inflight.max(1),
            batch_size: batch_size.max(1),
            max_tracked_ids,
            max_received_ids: max_tracked_ids,
            request_timeout: request_timeout.max(Duration::from_millis(1)),
            max_retries,
        }
    }

    pub(crate) fn enqueue(
        &mut self,
        chain_index: ChainIndex,
        transaction_id: TransactionId,
    ) -> bool {
        if transaction_id.bytes == [0; 32]
            || self.active_ids.contains(&transaction_id.bytes)
            || self.received_ids.contains(&transaction_id.bytes)
            || self.active_ids.len() >= self.max_tracked_ids
        {
            return false;
        }

        self.active_ids.insert(transaction_id.bytes);
        self.pending.push_back(TrackedTransaction {
            chain_index,
            transaction_id,
            retries: 0,
        });
        true
    }

    pub(crate) fn build_ready_requests(
        &mut self,
        next_request_id: &mut u32,
        now: Instant,
    ) -> Vec<TxsRequest> {
        let mut requests = Vec::new();
        while self.inflight.len() < self.max_inflight && !self.pending.is_empty() {
            let mut transactions = Vec::with_capacity(self.batch_size);
            for _ in 0..self.batch_size {
                let Some(transaction) = self.pending.pop_front() else {
                    break;
                };
                transactions.push(transaction);
            }
            if transactions.is_empty() {
                break;
            }

            let request_id = allocate_request_id(next_request_id, &self.inflight);
            let mut hashes: Vec<(ChainIndex, Vec<TransactionId>)> = Vec::new();
            for transaction in &transactions {
                if let Some((_, chain_hashes)) = hashes.iter_mut().find(|(chain, _)| {
                    chain.from == transaction.chain_index.from
                        && chain.to == transaction.chain_index.to
                }) {
                    chain_hashes.push(transaction.transaction_id);
                } else {
                    hashes.push((transaction.chain_index, vec![transaction.transaction_id]));
                }
            }

            let previous = self.inflight.insert(
                request_id,
                InflightRequest {
                    sent_at: now,
                    transactions,
                },
            );
            debug_assert!(previous.is_none());
            requests.push(TxsRequest {
                id: RequestId { value: request_id },
                hashes,
            });
        }
        requests
    }

    pub(crate) fn complete(
        &mut self,
        request_id: u32,
        raw_transactions: Vec<Vec<u8>>,
    ) -> Completion {
        let Some(mut request) = self.inflight.remove(&request_id) else {
            return Completion {
                unknown_request: true,
                ..Completion::default()
            };
        };

        let mut completion = Completion::default();
        for raw_transaction in raw_transactions {
            let transaction_id = hash_blake2b_256(&raw_transaction);
            let matching_index = request
                .transactions
                .iter()
                .position(|tracked| tracked.transaction_id.bytes == transaction_id);
            let chain_index =
                matching_index.map(|index| request.transactions.swap_remove(index).chain_index);

            self.active_ids.remove(&transaction_id);
            self.mark_received(transaction_id);
            completion.transactions.push(CompletedTransaction {
                chain_index,
                transaction_id,
                raw_transaction,
            });
        }

        for transaction in request.transactions {
            if self.retry_or_release(transaction) {
                completion.requeued += 1;
            } else {
                completion.exhausted += 1;
            }
        }
        completion
    }

    pub(crate) fn expire(&mut self, now: Instant) -> (usize, usize) {
        let expired_ids = self
            .inflight
            .iter()
            .filter_map(|(request_id, request)| {
                (now.saturating_duration_since(request.sent_at) >= self.request_timeout)
                    .then_some(*request_id)
            })
            .collect::<Vec<_>>();

        let mut requeued = 0;
        let mut exhausted = 0;
        for request_id in expired_ids {
            if let Some(request) = self.inflight.remove(&request_id) {
                for transaction in request.transactions {
                    if self.retry_or_release(transaction) {
                        requeued += 1;
                    } else {
                        exhausted += 1;
                    }
                }
            }
        }
        (requeued, exhausted)
    }

    fn retry_or_release(&mut self, mut transaction: TrackedTransaction) -> bool {
        if transaction.retries < self.max_retries {
            transaction.retries = transaction.retries.saturating_add(1);
            self.pending.push_back(transaction);
            true
        } else {
            self.active_ids.remove(&transaction.transaction_id.bytes);
            false
        }
    }

    fn mark_received(&mut self, transaction_id: [u8; 32]) {
        if self.received_ids.insert(transaction_id) {
            self.received_order.push_back(transaction_id);
        }
        while self.received_order.len() > self.max_received_ids {
            if let Some(expired) = self.received_order.pop_front() {
                self.received_ids.remove(&expired);
            }
        }
    }
}

fn allocate_request_id(next_request_id: &mut u32, inflight: &HashMap<u32, InflightRequest>) -> u32 {
    loop {
        let candidate = *next_request_id;
        *next_request_id = (*next_request_id).wrapping_add(1);
        if *next_request_id == 0 {
            *next_request_id = 1;
        }
        if candidate != 0 && !inflight.contains_key(&candidate) {
            return candidate;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use alephium_crypto::hash_blake2b_256;
    use alephium_protocol::message::payload::{ChainIndex, TransactionId};

    use super::TxRequestTracker;

    fn transaction_id(value: u8) -> TransactionId {
        TransactionId { bytes: [value; 32] }
    }

    #[test]
    fn deduplicates_announcements() {
        let mut tracker = TxRequestTracker::new(2, 8, 64, Duration::from_millis(50), 1);
        let chain = ChainIndex { from: 0, to: 1 };
        assert!(tracker.enqueue(chain, transaction_id(1)));
        assert!(!tracker.enqueue(chain, transaction_id(1)));
    }

    #[test]
    fn builds_multiple_inflight_requests() {
        let mut tracker = TxRequestTracker::new(2, 1, 64, Duration::from_millis(50), 1);
        let chain = ChainIndex { from: 0, to: 1 };
        assert!(tracker.enqueue(chain, transaction_id(1)));
        assert!(tracker.enqueue(chain, transaction_id(2)));
        let mut next_id = 1;
        let requests = tracker.build_ready_requests(&mut next_id, Instant::now());
        assert_eq!(requests.len(), 2);
    }

    #[test]
    fn maps_raw_transaction_back_to_requested_chain() {
        let raw = vec![1, 2, 3, 4];
        let id = hash_blake2b_256(&raw);
        let mut tracker = TxRequestTracker::new(1, 8, 64, Duration::from_millis(50), 1);
        let chain = ChainIndex { from: 2, to: 3 };
        assert!(tracker.enqueue(chain, TransactionId { bytes: id }));
        let mut next_id = 1;
        let request = tracker
            .build_ready_requests(&mut next_id, Instant::now())
            .pop()
            .expect("request");
        let completion = tracker.complete(request.id.value, vec![raw]);
        assert_eq!(completion.transactions.len(), 1);
        assert_eq!(completion.transactions[0].chain_index, Some(chain));
    }
}
