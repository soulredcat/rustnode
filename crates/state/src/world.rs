//! `WorldState.Cached`: the output, contract and code tries of one group,
//! changed in memory on top of a stored world state.

use std::collections::HashMap;

use alephium_model::{Hash, OutputRef, TxOutput, serialize};

use crate::store::Roots;
use crate::trie::{NodeSource, Trie};

pub struct WorldState<'a, S: NodeSource> {
    pub outputs: Trie<'a, S>,
    pub contracts: Trie<'a, S>,
    pub code: Trie<'a, S>,
    /// Contract immutable states and code written since the state was
    /// opened (`contractImmutableState`), stored beside the trie nodes.
    immutable: HashMap<Hash, Vec<u8>>,
    source: &'a S,
}

impl<'a, S: NodeSource> WorldState<'a, S> {
    pub fn new(roots: &Roots, source: &'a S) -> Self {
        Self {
            outputs: Trie::new(roots.outputs, source),
            contracts: Trie::new(roots.contracts, source),
            code: Trie::new(roots.code, source),
            immutable: HashMap::new(),
            source,
        }
    }

    /// A contract immutable state or code entry (`Either` tag included).
    pub fn immutable(&self, hash: &Hash) -> Result<Option<Vec<u8>>, String> {
        match self.immutable.get(hash) {
            Some(bytes) => Ok(Some(bytes.clone())),
            None => self.source.node(hash),
        }
    }

    pub fn put_immutable(&mut self, hash: Hash, bytes: Vec<u8>) {
        self.immutable.insert(hash, bytes);
    }

    #[must_use]
    pub const fn roots(&self) -> Roots {
        Roots {
            outputs: self.outputs.root(),
            contracts: self.contracts.root(),
            code: self.code.root(),
        }
    }

    /// `existOutput`.
    pub fn has_output(&self, output_ref: &OutputRef) -> Result<bool, String> {
        Ok(self.outputs.get(&serialize(output_ref))?.is_some())
    }

    /// `removeAsset` / the VM's removal of a contract's spent output.
    pub fn remove_output(&mut self, output_ref: &OutputRef) -> Result<(), String> {
        self.outputs.remove(&serialize(output_ref))
    }

    /// `addAsset` / the VM's new contract output.
    pub fn put_output(&mut self, output_ref: &OutputRef, output: &TxOutput) -> Result<(), String> {
        self.outputs.put(&serialize(output_ref), &serialize(output))
    }

    /// The nodes of every trie, and the immutable entries, created since the
    /// state was opened.
    pub fn new_nodes(&self) -> impl Iterator<Item = (&Hash, &Vec<u8>)> {
        self.outputs
            .new_nodes()
            .iter()
            .chain(self.contracts.new_nodes())
            .chain(self.code.new_nodes())
            .chain(self.immutable.iter())
    }
}
