#![forbid(unsafe_code)]
//! Official Alephium protocol models and messages.
//!
//! The current implementation covers the M1 message envelope and raw message
//! header/code boundary. Payload models, networking, validation, transactions,
//! blocks, and VM semantics are not implemented yet.

pub mod message;

/// Current implementation status.
pub const STATUS: &str =
    "M1 frame codec implemented; payload models and P2P networking are not implemented";
