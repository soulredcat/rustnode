//! `SparseMerkleTrie` nodes. A branch holds a nibble path and 16 optional
//! children, a leaf a nibble path and its value; every node is stored under
//! the Blake2b hash of its encoding, so a root hash fixes the whole trie.
//!
//! Encoding (`SparseMerkleTrie.Node.SerdeNode`): a compact int flag
//! `path length << 1 | (0 for a leaf, 1 for a branch)`, the path packed two
//! nibbles per byte, then 16 optional child hashes or the value as bytes.

use alephium_model::{Bytes, Codec, Hash, Reader};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Branch {
        path: Vec<u8>,
        children: Box<[Option<Hash>; 16]>,
    },
    Leaf {
        path: Vec<u8>,
        data: Vec<u8>,
    },
}

impl Node {
    /// The node encoded in `bytes`, which it must fill exactly.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader::new(bytes);
        let flag = reader.int().map_err(|error| error.to_string())?;
        let flag = u32::try_from(flag).map_err(|_| format!("negative trie node flag {flag}"))?;
        let (length, leaf) = ((flag >> 1) as usize, flag & 1 == 0);
        let packed = reader
            .take(length.div_ceil(2))
            .map_err(|error| error.to_string())?;
        let path = (0..length)
            .map(|i| {
                let byte = packed[i / 2];
                if i % 2 == 0 { byte >> 4 } else { byte & 0x0f }
            })
            .collect();
        let node = if leaf {
            let data = Bytes::decode(&mut reader).map_err(|error| error.to_string())?;
            Self::Leaf { path, data: data.0 }
        } else {
            let mut children = Box::new([None; 16]);
            for child in children.iter_mut() {
                *child = Option::<Hash>::decode(&mut reader).map_err(|error| error.to_string())?;
            }
            Self::Branch { path, children }
        };
        if reader.remaining() != 0 {
            return Err("trailing bytes after a trie node".to_owned());
        }
        Ok(node)
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let (path, leaf) = match self {
            Self::Branch { path, .. } => (path, false),
            Self::Leaf { path, .. } => (path, true),
        };
        let mut out = Vec::new();
        let flag = (path.len() << 1) | usize::from(!leaf);
        i32::try_from(flag).unwrap_or(i32::MAX).encode(&mut out);
        out.extend(
            path.chunks(2)
                .map(|pair| (pair[0] << 4) | pair.get(1).copied().unwrap_or(0)),
        );
        match self {
            Self::Branch { children, .. } => {
                for child in children.iter() {
                    child.encode(&mut out);
                }
            }
            Self::Leaf { data, .. } => {
                i32::try_from(data.len())
                    .unwrap_or(i32::MAX)
                    .encode(&mut out);
                out.extend_from_slice(data);
            }
        }
        out
    }

    #[must_use]
    pub fn path(&self) -> &[u8] {
        match self {
            Self::Branch { path, .. } | Self::Leaf { path, .. } => path,
        }
    }

    /// `preExtend`: the same node under a longer path.
    fn pre_extend(self, prefix: &[u8]) -> Self {
        let extend = |path: Vec<u8>| [prefix, &path].concat();
        match self {
            Self::Branch { path, children } => Self::Branch {
                path: extend(path),
                children,
            },
            Self::Leaf { path, data } => Self::Leaf {
                path: extend(path),
                data,
            },
        }
    }

    /// `preCut`: the same node without the first `n` nibbles of its path.
    fn pre_cut(self, n: usize) -> Self {
        match self {
            Self::Branch { path, children } => Self::Branch {
                path: path[n..].to_vec(),
                children,
            },
            Self::Leaf { path, data } => Self::Leaf {
                path: path[n..].to_vec(),
                data,
            },
        }
    }
}

/// Where trie nodes are read from.
pub trait NodeSource {
    fn node(&self, hash: &Hash) -> Result<Option<Vec<u8>>, String>;
}

/// A trie being changed: new nodes stay in memory until they are written
/// out, and old nodes are read through `source`. Its shape depends only on
/// its keys and values, as the official `SparseMerkleTrie`'s does, so the
/// root hash matches the official one for the same contents.
pub struct Trie<'a, S: NodeSource> {
    root: Hash,
    source: &'a S,
    new_nodes: std::collections::HashMap<Hash, Vec<u8>>,
}

impl<'a, S: NodeSource> Trie<'a, S> {
    pub fn new(root: Hash, source: &'a S) -> Self {
        Self {
            root,
            source,
            new_nodes: std::collections::HashMap::new(),
        }
    }

    #[must_use]
    pub const fn root(&self) -> Hash {
        self.root
    }

    /// The nodes created since the trie was opened, by hash; some may no
    /// longer be reachable from the root.
    #[must_use]
    pub fn new_nodes(&self) -> &std::collections::HashMap<Hash, Vec<u8>> {
        &self.new_nodes
    }

    fn load(&self, hash: &Hash) -> Result<Node, String> {
        let bytes = match self.new_nodes.get(hash) {
            Some(bytes) => bytes.clone(),
            None => self
                .source
                .node(hash)?
                .ok_or_else(|| format!("missing trie node {}", hash.to_hex()))?,
        };
        Node::decode(&bytes)
    }

    fn store(&mut self, node: &Node) -> Hash {
        let bytes = node.encode();
        let hash = Hash::blake2b(&bytes);
        self.new_nodes.insert(hash, bytes);
        hash
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
        get(&self.root, key, |hash| match self.new_nodes.get(hash) {
            Some(bytes) => Ok(Some(bytes.clone())),
            None => self.source.node(hash),
        })
    }

    /// `getAll(prefix)`: every key starting with `prefix` and its value, in
    /// key order.
    pub fn get_all(&self, prefix: &[u8]) -> Result<Vec<Entry>, String> {
        let mut out = Vec::new();
        self.collect(self.root, Vec::new(), &nibbles(prefix), &mut out)?;
        Ok(out)
    }

    fn collect(
        &self,
        hash: Hash,
        consumed: Vec<u8>,
        prefix: &[u8],
        out: &mut Vec<Entry>,
    ) -> Result<(), String> {
        let node = self.load(&hash)?;
        let path = node.path().to_vec();
        let common = path.len().min(prefix.len());
        if path[..common] != prefix[..common] {
            return Ok(());
        }
        let mut here = consumed;
        here.extend_from_slice(&path);
        match node {
            Node::Leaf { data, .. } => {
                if path.len() >= prefix.len() {
                    out.push((pack(&here), data));
                }
            }
            Node::Branch { children, .. } => {
                let rest = &prefix[common..];
                for (nibble, child) in children.iter().enumerate() {
                    let Some(child) = child else { continue };
                    let nibble = u8::try_from(nibble).unwrap_or(0);
                    if let Some((first, tail)) = rest.split_first() {
                        if *first == nibble {
                            self.collect(*child, [&here[..], &[nibble]].concat(), tail, out)?;
                        }
                    } else {
                        self.collect(*child, [&here[..], &[nibble]].concat(), &[], out)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// `putRaw`.
    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), String> {
        let (_, hash) = self.put_at(self.root, &nibbles(key), value)?;
        self.root = hash;
        Ok(())
    }

    /// `removeRaw`; an absent key is an error, as in the official trie.
    pub fn remove(&mut self, key: &[u8]) -> Result<(), String> {
        if let Some((_, hash)) = self.remove_at(self.root, &nibbles(key))? {
            self.root = hash;
        }
        Ok(())
    }

    fn put_at(&mut self, hash: Hash, nibbles: &[u8], value: &[u8]) -> Result<(Node, Hash), String> {
        let node = self.load(&hash)?;
        let path = node.path().to_vec();
        if path.len() > nibbles.len() {
            return Err("trie key shorter than a node path".to_owned());
        }
        if let Some(index) = (0..path.len()).find(|i| nibbles[*i] != path[*i]) {
            // `branch`: the node and a new leaf under a branch at the fork.
            let new_leaf = Node::Leaf {
                path: nibbles[index + 1..].to_vec(),
                data: value.to_vec(),
            };
            let (nibble1, nibble2) = (path[index], nibbles[index]);
            let cut = node.pre_cut(index + 1);
            let mut children = Box::new([None; 16]);
            children[nibble1 as usize] = Some(self.store(&cut));
            children[nibble2 as usize] = Some(self.store(&new_leaf));
            let branch = Node::Branch {
                path: path[..index].to_vec(),
                children,
            };
            let hash = self.store(&branch);
            return Ok((branch, hash));
        }
        match node {
            Node::Branch { path, children } => {
                let nibble = *nibbles.get(path.len()).ok_or("trie key ends at a branch")?;
                let rest = &nibbles[path.len() + 1..];
                let new_child = match children[nibble as usize] {
                    Some(child) => self.put_at(child, rest, value)?.1,
                    None => self.store(&Node::Leaf {
                        path: rest.to_vec(),
                        data: value.to_vec(),
                    }),
                };
                Ok(self
                    .replace_child(hash, path, children, nibble, Some(new_child))?
                    .ok_or("a put emptied a branch")?)
            }
            Node::Leaf { path, data } => {
                if path.len() != nibbles.len() {
                    return Err("trie key longer than a leaf path".to_owned());
                }
                if data == value {
                    return Ok((Node::Leaf { path, data }, hash));
                }
                let leaf = Node::Leaf {
                    path,
                    data: value.to_vec(),
                };
                let hash = self.store(&leaf);
                Ok((leaf, hash))
            }
        }
    }

    fn remove_at(&mut self, hash: Hash, nibbles: &[u8]) -> Result<Option<(Node, Hash)>, String> {
        let not_found = || format!("trie key not found under {}", hash.to_hex());
        match self.load(&hash)? {
            Node::Branch { path, children } => {
                let nibble = *nibbles.get(path.len()).ok_or_else(not_found)?;
                let child = children[nibble as usize].ok_or_else(not_found)?;
                if nibbles[..path.len()] != path[..] {
                    return Err(not_found());
                }
                let new_child = self
                    .remove_at(child, &nibbles[path.len() + 1..])?
                    .map(|(_, hash)| hash);
                self.replace_child(hash, path, children, nibble, new_child)
            }
            Node::Leaf { path, .. } if path == nibbles => Ok(None),
            Node::Leaf { .. } => Err(not_found()),
        }
    }

    /// `handleChildUpdateResult`: a branch left with one child becomes that
    /// child under the branch's path and nibble.
    fn replace_child(
        &mut self,
        hash: Hash,
        path: Vec<u8>,
        children: Box<[Option<Hash>; 16]>,
        nibble: u8,
        new_child: Option<Hash>,
    ) -> Result<Option<(Node, Hash)>, String> {
        let mut new_children = children.clone();
        new_children[nibble as usize] = new_child;
        if new_child.is_none() && new_children.iter().flatten().count() == 1 {
            let only = new_children
                .iter()
                .position(Option::is_some)
                .ok_or("no child left")?;
            let only_hash = children[only].ok_or("no child left")?;
            let extended = self
                .load(&only_hash)?
                .pre_extend(&[&path[..], &[u8::try_from(only).unwrap_or(0)]].concat());
            let hash = self.store(&extended);
            return Ok(Some((extended, hash)));
        }
        if children[nibble as usize] == new_child {
            return Ok(Some((Node::Branch { path, children }, hash)));
        }
        let branch = Node::Branch {
            path,
            children: new_children,
        };
        let hash = self.store(&branch);
        Ok(Some((branch, hash)))
    }
}

/// The nibbles of a key, high nibble first (`bytes2Nibbles`).
#[must_use]
pub fn nibbles(key: &[u8]) -> Vec<u8> {
    key.iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .collect()
}

/// A key and its value.
pub type Entry = (Vec<u8>, Vec<u8>);

/// Nibbles back to bytes, high nibble first.
fn pack(nibbles: &[u8]) -> Vec<u8> {
    nibbles
        .chunks(2)
        .map(|pair| (pair[0] << 4) | pair.get(1).copied().unwrap_or(0))
        .collect()
}

/// The value under `key` in the trie of `root`, reading nodes through
/// `node`; `Ok(None)` when the key is absent.
pub fn get(
    root: &Hash,
    key: &[u8],
    mut node: impl FnMut(&Hash) -> Result<Option<Vec<u8>>, String>,
) -> Result<Option<Vec<u8>>, String> {
    let key = nibbles(key);
    let mut rest: &[u8] = &key;
    let mut hash = *root;
    loop {
        let bytes = node(&hash)?.ok_or_else(|| format!("missing trie node {}", hash.to_hex()))?;
        let decoded = Node::decode(&bytes)?;
        let Some(after) = rest.strip_prefix(decoded.path()) else {
            return Ok(None);
        };
        match decoded {
            Node::Leaf { data, .. } => return Ok(after.is_empty().then_some(data)),
            Node::Branch { children, .. } => {
                let Some((nibble, tail)) = after.split_first() else {
                    return Ok(None);
                };
                let Some(child) = children[*nibble as usize] else {
                    return Ok(None);
                };
                hash = child;
                rest = tail;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alephium_model::Hash;

    use super::{Node, get, nibbles};

    /// A leaf `[1, 2, 3]` with value `[9, 9]`: flag 3 << 1, the path packed
    /// as 0x12 0x30, then the value with its length.
    const LEAF: [u8; 6] = [0x06, 0x12, 0x30, 0x02, 0x09, 0x09];

    #[test]
    fn leaves_and_branches_decode() {
        assert_eq!(
            Node::decode(&LEAF),
            Ok(Node::Leaf {
                path: vec![1, 2, 3],
                data: vec![9, 9]
            })
        );
        let mut branch = vec![0x01];
        branch.extend([0u8; 15]);
        branch.push(1);
        branch.extend([7u8; 32]);
        let mut children = Box::new([None; 16]);
        children[15] = Some(Hash([7; 32]));
        assert_eq!(
            Node::decode(&branch),
            Ok(Node::Branch {
                path: Vec::new(),
                children
            })
        );
        assert!(Node::decode(&[0x06, 0x12, 0x30, 0x02, 0x09, 0x09, 0x00]).is_err());
    }

    #[test]
    fn lookups_follow_the_path() {
        let leaf_hash = Hash::blake2b(&LEAF);
        let mut root = vec![0x01];
        for nibble in 0..16 {
            if nibble == 0xa {
                root.push(1);
                root.extend(leaf_hash.0);
            } else {
                root.push(0);
            }
        }
        let root_hash = Hash::blake2b(&root);
        let read = |hash: &Hash| {
            Ok(if *hash == root_hash {
                Some(root.clone())
            } else if *hash == leaf_hash {
                Some(LEAF.to_vec())
            } else {
                None
            })
        };
        assert_eq!(nibbles(&[0xa1, 0x23]), vec![0xa, 1, 2, 3]);
        assert_eq!(get(&root_hash, &[0xa1, 0x23], read), Ok(Some(vec![9, 9])));
        assert_eq!(get(&root_hash, &[0xa1, 0x24], read), Ok(None));
        assert_eq!(get(&root_hash, &[0xb1, 0x23], read), Ok(None));
    }
}
