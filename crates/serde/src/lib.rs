#![forbid(unsafe_code)]
//! Exact Alephium binary serialization and deserialization.
//!
//! This crate reproduces the official binary format. It is not an alias for
//! the general-purpose Rust `serde` data model.

pub mod bounded;
pub mod compact;
pub mod error;
pub mod fixed;
pub mod staging;

pub use error::Error;
pub use staging::{DecodeResult, Staging};

/// Current implementation status.
pub const STATUS: &str =
    "M1 primitive codec implemented; message framing and payloads are not implemented";
