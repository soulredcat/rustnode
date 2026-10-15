#![forbid(unsafe_code)]
//! Alephium protocol model types with the official binary encoding: blocks,
//! headers, transactions, lockup and unlock scripts, VM scripts and values.
//!
//! The pinned official implementation (`UPSTREAM.toml`) is the oracle; every
//! type is checked against blocks produced by the official node.

pub mod address;
pub mod block;
pub mod codec;
pub mod hash;
pub mod instr;
mod instr_table;
pub mod lockup;
pub mod network;
pub mod script;
pub mod tx;
pub mod val;

pub use block::{Block, BlockHeader, Target};
pub use codec::{Bytes, Codec, Reader, TimeStamp, deserialize, serialize};
pub use ethnum::{I256, U256};
pub use hash::Hash;
pub use instr::{Context, Instr, Operand};
pub use lockup::{LockupScript, PublicKeyLike, UnlockScript};
pub use script::{Method, StatefulScript, StatelessScript};
pub use tx::{
    AssetOutput, ContractOutput, OutputRef, Transaction, TransactionTemplate, TxInput, TxOutput,
    UnsignedTransaction,
};
pub use val::Val;
