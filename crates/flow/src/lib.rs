#![forbid(unsafe_code)]
//! Alephium P2P runtime for the `nixiaproject`-only node.
//!
//! It owns:
//! - official Alephium P2P framing and session liveness;
//! - immediate transaction-announcement fetch with bounded concurrency; and
//! - the mempool `GET /mempool/transactions` is served from, kept against the
//!   best view of every chain index; and
//! - the world state bootstrap from the official node's database.

pub mod bootstrap;
mod mempool;
mod request_tracker;
mod session;
pub mod state;
pub mod sync;

pub use mempool::{Mempool, MempoolTx, SharedMempool};
pub use session::{FastPathError, STATUS, run};
pub use sync::{BlockSync, SharedChainStore};
