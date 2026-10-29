#![forbid(unsafe_code)]
//! The world state: `SparseMerkleTrie` nodes and contract data on disk, and
//! the import of a block's world state from an official node's database.

mod import;
mod store;
pub mod trie;

pub use import::{Copied, Official, copy_state};
pub use store::{Roots, StateStore};
