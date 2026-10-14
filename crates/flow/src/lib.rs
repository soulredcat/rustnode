#![forbid(unsafe_code)]
//! Alephium P2P runtime for the `nixiaproject`-only node.
//!
//! It owns:
//! - official Alephium P2P framing and session liveness;
//! - immediate transaction-announcement fetch with bounded concurrency; and
//! - the bounded raw mempool that `GET /mempool/transactions` will be served from.

mod mempool;
mod request_tracker;
mod session;

pub use mempool::{RawMempool, RawMempoolTx, SharedRawMempool};
pub use session::{FastPathError, STATUS, run};
