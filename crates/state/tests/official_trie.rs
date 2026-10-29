//! Trie mutations against the official node's own tries: the leaves that
//! differ between the world state after a block and after its parent,
//! applied to the parent's tries, must give the block's roots; and every
//! node read on the way must re-encode to its stored bytes.
//!
//! ```text
//! RUSTNODE_OFFICIAL_DB=<official db folder> \
//!   cargo test -p alephium-state --test official_trie -- --ignored
//! ```

use std::collections::HashMap;

use alephium_model::Hash;
use alephium_state::Official;
use alephium_state::trie::{Node, NodeSource, Trie};

const GROUPS: u32 = 4;

fn official_url() -> String {
    std::env::var("RUSTNODE_OFFICIAL_URL").unwrap_or_else(|_| "http://127.0.0.1:12973".to_owned())
}

fn rest(path: &str) -> serde_json::Value {
    let body = ureq::get(&format!("{}{path}", official_url()))
        .call()
        .expect("official REST")
        .into_string()
        .expect("body");
    serde_json::from_str(&body).expect("JSON")
}

fn load(source: &Official, hash: &Hash) -> Node {
    let bytes = source.node(hash).expect("read").expect("node present");
    assert_eq!(Hash::blake2b(&bytes), *hash);
    let node = Node::decode(&bytes).expect("decode");
    assert_eq!(node.encode(), bytes, "re-encoding {}", hash.to_hex());
    node
}

fn leaves(source: &Official, hash: Hash, base: &[u8], out: &mut HashMap<Vec<u8>, Vec<u8>>) {
    match load(source, &hash) {
        Node::Leaf { path, data } => {
            out.insert([base, &path].concat(), data);
        }
        Node::Branch { path, children } => {
            for (nibble, child) in children.iter().enumerate() {
                if let Some(child) = child {
                    let prefix = [base, &path, &[nibble as u8]].concat();
                    leaves(source, *child, &prefix, out);
                }
            }
        }
    }
}

/// Keys whose value differs from `old` to `new` (puts) and keys only in
/// `old` (removes), as nibbles; equal subtrees at the same place are skipped.
fn diff(
    source: &Official,
    old: Option<Hash>,
    new: Option<Hash>,
    base: &[u8],
    puts: &mut Vec<(Vec<u8>, Vec<u8>)>,
    removes: &mut Vec<Vec<u8>>,
) {
    if old == new {
        return;
    }
    if let (Some(old_hash), Some(new_hash)) = (old, new)
        && let (
            Node::Branch {
                path: old_path,
                children: old_children,
            },
            Node::Branch {
                path: new_path,
                children: new_children,
            },
        ) = (load(source, &old_hash), load(source, &new_hash))
        && old_path == new_path
    {
        for nibble in 0..16 {
            let prefix = [base, &old_path, &[nibble as u8]].concat();
            diff(
                source,
                old_children[nibble],
                new_children[nibble],
                &prefix,
                puts,
                removes,
            );
        }
        return;
    }
    let (mut old_leaves, mut new_leaves) = (HashMap::new(), HashMap::new());
    if let Some(old) = old {
        leaves(source, old, base, &mut old_leaves);
    }
    if let Some(new) = new {
        leaves(source, new, base, &mut new_leaves);
    }
    for (key, value) in new_leaves {
        if old_leaves.remove(&key).as_ref() != Some(&value) {
            puts.push((key, value));
        }
    }
    removes.extend(old_leaves.into_keys());
}

fn bytes(nibbles: &[u8]) -> Vec<u8> {
    nibbles
        .chunks(2)
        .map(|pair| (pair[0] << 4) | pair[1])
        .collect()
}

/// Consecutive blocks per intra-group chain, walked back from 10 below
/// the tip.
const PAIRS: usize = 30;

#[test]
#[ignore = "needs the operator's official node database and REST API"]
fn block_diffs_reproduce_official_roots() {
    let db = std::env::var("RUSTNODE_OFFICIAL_DB").expect("RUSTNODE_OFFICIAL_DB");
    let secondary =
        std::env::temp_dir().join(format!("rustnode-official-trie-{}", std::process::id()));
    let official = Official::open(db.as_ref(), &secondary).expect("open official database");
    let mut totals: HashMap<&str, (usize, usize)> = HashMap::new();
    for group in 0..GROUPS {
        let height = rest(&format!(
            "/blockflow/chain-info?fromGroup={group}&toGroup={group}"
        ))["currentHeight"]
            .as_u64()
            .expect("height");
        let mut block = rest(&format!(
            "/blockflow/hashes?fromGroup={group}&toGroup={group}&height={}",
            height - 10
        ))["headers"][0]
            .as_str()
            .expect("hash")
            .to_owned();
        for _ in 0..PAIRS {
            let parent = rest(&format!("/blockflow/blocks/{block}"))["deps"]
                [(GROUPS - 1 + group) as usize]
                .as_str()
                .expect("parent")
                .to_owned();
            let new = official
                .roots(&Hash::from_hex(&block).expect("block hash"))
                .expect("read")
                .expect("roots of the block");
            let old = official
                .roots(&Hash::from_hex(&parent).expect("parent hash"))
                .expect("read")
                .expect("roots of the parent");
            for (name, old_root, new_root) in [
                ("outputs", old.outputs, new.outputs),
                ("contracts", old.contracts, new.contracts),
                ("code", old.code, new.code),
            ] {
                let (mut puts, mut removes) = (Vec::new(), Vec::new());
                diff(
                    &official,
                    Some(old_root),
                    Some(new_root),
                    &[],
                    &mut puts,
                    &mut removes,
                );
                let mut trie = Trie::new(old_root, &official);
                for key in &removes {
                    trie.remove(&bytes(key)).expect("remove");
                }
                for (key, value) in &puts {
                    trie.put(&bytes(key), value).expect("put");
                }
                assert_eq!(
                    trie.root(),
                    new_root,
                    "group {group} {name}: {} puts and {} removes from {parent} to {block}",
                    puts.len(),
                    removes.len(),
                );
                let total = totals.entry(name).or_default();
                total.0 += puts.len();
                total.1 += removes.len();
            }
            block = parent;
        }
    }
    let mut names: Vec<_> = totals.into_iter().collect();
    names.sort_unstable();
    for (name, (puts, removes)) in &names {
        println!(
            "{name}: {puts} puts and {removes} removes over {} blocks reproduce the official roots",
            PAIRS * GROUPS as usize
        );
    }
    // The walk must have exercised removals and contract updates.
    assert!(
        names
            .iter()
            .any(|(name, (_, removes))| *name == "outputs" && *removes > 0)
    );
    assert!(
        names
            .iter()
            .any(|(name, (puts, _))| *name == "contracts" && *puts > 0)
    );
    drop(official);
    let _ = std::fs::remove_dir_all(secondary);
}
