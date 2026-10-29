#![forbid(unsafe_code)]
//! The world state: `SparseMerkleTrie` nodes and contract data on disk, and
//! the import of a block's world state from an official node's database.

mod events;
mod import;
mod prune;
mod store;
pub mod trie;
mod world;

pub use events::{LogState, LogStates, NewEvent};
pub use import::{Copied, Official, copy_state};
pub use prune::Pruned;
pub use store::{Roots, StateStore};
pub use world::WorldState;
