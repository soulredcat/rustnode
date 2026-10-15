//! `BlockHeader` and `Block` (`protocol/model/{BlockHeader,Block,Target}.scala`,
//! `protocol/mining/PoW.scala`).

use alephium_serde::Error;
use ethnum::U256;

use crate::codec::{Codec, Reader, TimeStamp, serialize};
use crate::hash::Hash;
use crate::tx::Transaction;

/// `Target`: 4-byte compact bits, `mantissa * 256^(size - 3)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Target(pub [u8; 4]);

impl Target {
    /// The target value, or `None` when it reaches `2^256` (every hash passes).
    #[must_use]
    pub fn value(&self) -> Option<U256> {
        let size = u32::from(self.0[0]);
        let mantissa = U256::from(u32::from_be_bytes([0, self.0[1], self.0[2], self.0[3]]));
        if size <= 3 {
            return Some(mantissa >> (8 * (3 - size)));
        }
        let shift = 8 * (size - 3);
        if mantissa == U256::ZERO {
            return Some(U256::ZERO);
        }
        // mantissa < 2^24, so it fits while shift + 24 <= 256 bits of headroom.
        if shift >= 256 || mantissa.leading_zeros() < shift {
            None
        } else {
            Some(mantissa << shift)
        }
    }
}

impl Codec for Target {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self(reader.array()?))
    }
}

pub const NONCE_LENGTH: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockHeader {
    pub nonce: [u8; NONCE_LENGTH],
    pub version: u8,
    /// `2 * groups - 1` hashes: the other groups' tips, then this group's
    /// chain tips (`BlockDeps`).
    pub deps: Vec<Hash>,
    pub dep_state_hash: Hash,
    pub txs_hash: Hash,
    pub timestamp: TimeStamp,
    pub target: Target,
}

impl BlockHeader {
    /// `PoW.hash`: double Blake3 of the serialized header.
    #[must_use]
    pub fn hash(&self) -> Hash {
        Hash::blake3_double(&serialize(self))
    }

    /// `ChainIndex.from(hash, groups)`: the last two hash bytes modulo the chain count.
    #[must_use]
    pub fn chain_index(hash: &Hash, groups: u32) -> (u32, u32) {
        let big_index = (u32::from(hash.0[30]) << 8) | u32::from(hash.0[31]);
        let index = big_index % (groups * groups);
        (index / groups, index % groups)
    }

    /// `PoW.checkWork`: the hash, as a big-endian number, is at most the target.
    #[must_use]
    pub fn check_work(hash: &Hash, target: &Target) -> bool {
        target
            .value()
            .is_none_or(|target| U256::from_be_bytes(hash.0) <= target)
    }

    /// `BlockDeps.outDeps`: the tips of this group's chains.
    #[must_use]
    pub fn out_deps(&self) -> &[Hash] {
        &self.deps[self.deps.len() / 2..]
    }

    /// `BlockDeps.inDeps`.
    #[must_use]
    pub fn in_deps(&self) -> &[Hash] {
        &self.deps[..self.deps.len() / 2]
    }
}

impl Codec for BlockHeader {
    fn encode(&self, out: &mut Vec<u8>) {
        self.nonce.encode(out);
        self.version.encode(out);
        self.deps.encode(out);
        self.dep_state_hash.encode(out);
        self.txs_hash.encode(out);
        self.timestamp.encode(out);
        self.target.encode(out);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            nonce: reader.array()?,
            version: reader.byte()?,
            deps: Vec::decode(reader)?,
            dep_state_hash: Hash::decode(reader)?,
            txs_hash: Hash::decode(reader)?,
            timestamp: TimeStamp::decode(reader)?,
            target: Target::decode(reader)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}

impl Block {
    #[must_use]
    pub fn hash(&self) -> Hash {
        self.header.hash()
    }
}

impl Codec for Block {
    fn encode(&self, out: &mut Vec<u8>) {
        self.header.encode(out);
        self.transactions.encode(out);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            header: BlockHeader::decode(reader)?,
            transactions: Vec::decode(reader)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use ethnum::U256;

    use super::{BlockHeader, Target};
    use crate::hash::Hash;

    #[test]
    fn target_bits_expand_like_the_official_node() {
        assert_eq!(
            Target([3, 0x12, 0x34, 0x56]).value(),
            Some(U256::from(0x12_3456u32))
        );
        assert_eq!(
            Target([1, 0x12, 0x34, 0x56]).value(),
            Some(U256::from(0x12u32))
        );
        assert_eq!(
            Target([4, 0x12, 0x34, 0x56]).value(),
            Some(U256::from(0x1234_5600u32))
        );
        // 0x20ffffff: 0xffffff << 232, still below 2^256.
        assert_eq!(
            Target([0x20, 0xff, 0xff, 0xff]).value(),
            Some(U256::from(0xff_ffffu32) << 232)
        );
        assert_eq!(Target([0x21, 0xff, 0xff, 0xff]).value(), None);
        assert!(BlockHeader::check_work(
            &Hash([0xff; 32]),
            &Target([0x21, 1, 0, 0])
        ));
        assert!(!BlockHeader::check_work(
            &Hash([0xff; 32]),
            &Target([0x20, 0xff, 0xff, 0xff])
        ));
    }

    #[test]
    fn chain_index_uses_the_last_two_bytes() {
        let mut hash = Hash::ZERO;
        hash.0[30] = 0x01;
        hash.0[31] = 0x05;
        // 0x0105 = 261; 261 % 16 = 5 -> chain 1 -> 1.
        assert_eq!(BlockHeader::chain_index(&hash, 4), (1, 1));
    }
}
