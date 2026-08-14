#![forbid(unsafe_code)]
//! Alephium P2P transaction fast path for `alphrustalbitrage`.
//!
//! This crate is intentionally not a full node. It owns only:
//! - official Alephium P2P framing and session liveness;
//! - immediate transaction-announcement fetch with bounded concurrency; and
//! - versioned loopback IPC events for a local bot process.

mod event;
mod ipc;
mod request_tracker;
mod session;

pub use event::{
    BotEvent, EVENT_HEADER_LEN, EVENT_MAGIC, EVENT_PROTOCOL_VERSION, EventDecodeError,
    EventEncodeError, EventKind, decode_event, encode_event,
};
pub use ipc::{EventSink, IpcPublisher, IpcStats, PublishOutcome};
pub use session::{FastPathError, STATUS, run};
