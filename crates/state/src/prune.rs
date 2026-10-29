//! Pruning the world state to the block window: every node, immutable
//! entry and root not reachable from the states of the blocks kept is
//! deleted, so the state takes the size of the window, not of the chain's
//! history.

use std::collections::HashSet;

use alephium_model::{Hash, OutputRef, Val, deserialize};
use redb::ReadableDatabase;

use crate::store::{NODES, ROOTS, Roots, StateStore};
use crate::trie::Node;

/// What a prune removed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pruned {
    pub kept_nodes: usize,
    pub nodes: u64,
    pub roots: u64,
}

/// A hash's first 16 bytes: the marked set holds tens of millions of them,
/// and a collision only ever keeps a node that could go.
fn mark_key(hash: &[u8; 32]) -> u128 {
    let mut prefix = [0u8; 16];
    prefix.copy_from_slice(&hash[..16]);
    u128::from_be_bytes(prefix)
}

impl StateStore {
    /// Keeps the states of `blocks` and everything they reach; deletes the
    /// rest.
    pub fn prune(&self, blocks: &[Hash]) -> Result<Pruned, String> {
        let keep: HashSet<[u8; 32]> = blocks.iter().map(|block| block.0).collect();
        let mut marked: HashSet<u128> = HashSet::new();
        {
            let txn = self.db().begin_read().map_err(|error| error.to_string())?;
            let roots = txn.open_table(ROOTS).map_err(|error| error.to_string())?;
            let nodes = txn.open_table(NODES).map_err(|error| error.to_string())?;
            let node = |hash: &Hash| -> Result<Option<Vec<u8>>, String> {
                Ok(nodes
                    .get(hash.0)
                    .map_err(|error| error.to_string())?
                    .map(|value| value.value().to_vec()))
            };
            for block in blocks {
                let Some(stored) = roots.get(block.0).map_err(|error| error.to_string())? else {
                    continue;
                };
                let stored = Roots::from_bytes(&stored.value());
                for (root, contracts) in [
                    (stored.outputs, false),
                    (stored.contracts, true),
                    (stored.code, false),
                ] {
                    mark_trie(&node, root, contracts, &mut marked)?;
                }
            }
        }
        let txn = self.db().begin_write().map_err(|error| error.to_string())?;
        let mut pruned = Pruned {
            kept_nodes: marked.len(),
            ..Pruned::default()
        };
        {
            let mut roots = txn.open_table(ROOTS).map_err(|error| error.to_string())?;
            roots
                .retain(|block, _| {
                    let kept = keep.contains(&block);
                    pruned.roots += u64::from(!kept);
                    kept
                })
                .map_err(|error| error.to_string())?;
            let mut nodes = txn.open_table(NODES).map_err(|error| error.to_string())?;
            nodes
                .retain(|hash, _| {
                    let kept = marked.contains(&mark_key(&hash));
                    pruned.nodes += u64::from(!kept);
                    kept
                })
                .map_err(|error| error.to_string())?;
        }
        txn.commit().map_err(|error| error.to_string())?;
        Ok(pruned)
    }
}

fn mark_trie(
    node: &impl Fn(&Hash) -> Result<Option<Vec<u8>>, String>,
    root: Hash,
    contracts: bool,
    marked: &mut HashSet<u128>,
) -> Result<(), String> {
    let mut stack = vec![root];
    while let Some(hash) = stack.pop() {
        if !marked.insert(mark_key(&hash.0)) {
            continue;
        }
        let bytes = node(&hash)?.ok_or_else(|| format!("missing trie node {}", hash.to_hex()))?;
        match Node::decode(&bytes)? {
            Node::Branch { children, .. } => stack.extend(children.into_iter().flatten()),
            Node::Leaf { data, .. } if contracts => mark_contract(node, &data, marked)?,
            Node::Leaf { .. } => {}
        }
    }
    Ok(())
}

/// A contract state since Leman refers to its immutable state, which refers
/// to its code; both are kept beside the trie nodes.
fn mark_contract(
    node: &impl Fn(&Hash) -> Result<Option<Vec<u8>>, String>,
    state: &[u8],
    marked: &mut HashSet<u128>,
) -> Result<(), String> {
    let Ok((_, (_, immutable_hash))) = deserialize::<(Vec<Val>, (OutputRef, Hash))>(state) else {
        return Ok(());
    };
    if !marked.insert(mark_key(&immutable_hash.0)) {
        return Ok(());
    }
    let immutable = node(&immutable_hash)?
        .ok_or_else(|| format!("missing immutable state {}", immutable_hash.to_hex()))?;
    let (code_hash, _) =
        deserialize::<(Hash, (Hash, Vec<Val>))>(immutable.get(1..).unwrap_or_default())
            .map_err(|error| format!("immutable state {}: {error}", immutable_hash.to_hex()))?;
    marked.insert(mark_key(&code_hash.0));
    Ok(())
}

#[cfg(test)]
mod tests {
    use alephium_model::Hash;

    use crate::store::{Roots, StateStore};
    use crate::trie::{Node, Trie, nibbles};

    #[test]
    fn pruning_keeps_the_kept_states_whole() {
        let path = std::env::temp_dir().join(format!("rustnode-prune-{}.redb", std::process::id()));
        let store = StateStore::open(&path).expect("store");
        let first = Node::Leaf {
            path: nibbles(&[1; 4]),
            data: vec![1],
        };
        let first_root = Hash::blake2b(&first.encode());
        let roots = |root| Roots {
            outputs: root,
            contracts: root,
            code: root,
        };
        let (old_block, new_block) = (Hash([1; 32]), Hash([2; 32]));
        let first_bytes = first.encode();
        store
            .put_state(
                &old_block,
                &roots(first_root),
                [(&first_root, &first_bytes)].into_iter(),
            )
            .expect("old state");
        let mut trie = Trie::new(first_root, &store);
        trie.put(&[2; 4], &[2]).expect("put");
        trie.remove(&[1; 4]).expect("remove");
        trie.put(&[3; 4], &[3]).expect("put");
        let new_root = trie.root();
        let nodes = trie.new_nodes().clone();
        store
            .put_state(&new_block, &roots(new_root), nodes.iter())
            .expect("new state");

        let pruned = store.prune(&[new_block]).expect("prune");
        assert_eq!(pruned.roots, 1);
        assert!(pruned.nodes >= 1);
        assert!(store.roots(&old_block).expect("read").is_none());
        assert!(store.node(&first_root).expect("read").is_none());
        let trie = Trie::new(new_root, &store);
        assert_eq!(trie.get(&[2; 4]), Ok(Some(vec![2])));
        assert_eq!(trie.get(&[3; 4]), Ok(Some(vec![3])));
        assert_eq!(trie.get(&[1; 4]), Ok(None));
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
