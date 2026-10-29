#![forbid(unsafe_code)]
//! The Ralph VM of the current hard fork (Danube), for replaying the
//! transaction scripts of followed blocks. A block records whether each
//! script succeeded (`scriptExecutionOk`), its contract inputs and its
//! generated outputs, so scripts are replayed without gas metering and the
//! result is checked against what the block says.

mod arith;
mod balances;
pub mod contract;
mod exec;
mod hashes;

use std::cmp::Ordering;

pub use exec::{BlockEnv, Event, Execution, run_tx_script};

/// A script that failed here: a divergence from the official VM when the
/// block says it succeeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fail(pub String);

pub type VmResult<T> = Result<T, Fail>;

pub(crate) fn fail<T>(message: impl Into<String>) -> VmResult<T> {
    Err(Fail(message.into()))
}

/// `Bytes.byteStringOrdering`: lexicographic over signed bytes.
pub(crate) fn compare_bytes(a: &[u8], b: &[u8]) -> Ordering {
    a.iter()
        .map(|byte| i8::from_be_bytes([*byte]))
        .cmp(b.iter().map(|byte| i8::from_be_bytes([*byte])))
}
