//! `BlockFlowState.getHashesForUpdates`: the blocks whose transactions a
//! block of a group applies, in Danube, to the world state of its
//! intra-group dependency before its own (`updateWorldStateDanube`).

use alephium_model::Hash;

use crate::{AddError, ChainStore, GENESIS_TIMESTAMP};

impl ChainStore {
    /// `FlowTipsUtil.getInTipUnsafe`: what `dep`, a block of another group,
    /// saw of the chain into `group`.
    fn in_tip(&self, dep: &Hash, group: u32) -> Result<Hash, AddError> {
        let entry = self.entry(dep)?;
        if entry.block.header.timestamp == GENESIS_TIMESTAMP {
            return Ok(self.genesis[self.chain(entry.from, group)]);
        }
        Ok(if group == entry.to {
            *dep
        } else {
            entry.block.header.deps[(self.groups - 1 + group) as usize]
        })
    }

    /// `FlowTipsUtil.getInOutTips`: the chains into and out of `group` as
    /// `hash`, a block of `group`, saw them.
    fn in_out_tips(&self, hash: &Hash, group: u32) -> Result<Vec<Hash>, AddError> {
        let entry = self.entry(hash)?;
        let mut tips = if entry.block.header.timestamp == GENESIS_TIMESTAMP {
            (0..self.groups - 1)
                .map(|i| {
                    let from = if i < group { i } else { i + 1 };
                    self.genesis[self.chain(from, group)]
                })
                .collect()
        } else {
            entry.block.header.deps[..(self.groups - 1) as usize]
                .iter()
                .map(|dep| self.in_tip(dep, group))
                .collect::<Result<Vec<_>, _>>()?
        };
        tips.extend(self.out_tips(hash)?);
        Ok(tips)
    }

    /// `isInBlockFlowUnsafe`: whether `intra`, an intra-group block, is in
    /// the view of `checkpoint`.
    pub fn in_block_flow(&self, checkpoint: &Hash, intra: &Hash) -> Result<bool, AddError> {
        let (group, _) = alephium_model::BlockHeader::chain_index(intra, self.groups);
        let tip = self.group_tip(checkpoint, group)?;
        self.is_extending(&tip, intra)
    }

    /// The blocks in the view of `deps` that the intra-group dependency of
    /// `group` did not see, chain by chain, oldest first.
    pub fn hashes_for_updates(&self, group: u32, deps: &[Hash]) -> Result<Vec<Hash>, AddError> {
        let (in_deps, out_deps) = deps.split_at((self.groups - 1) as usize);
        let mut new_tips = in_deps
            .iter()
            .map(|dep| self.in_tip(dep, group))
            .collect::<Result<Vec<_>, _>>()?;
        new_tips.extend_from_slice(out_deps);
        let old_tips = self.in_out_tips(&out_deps[group as usize], group)?;
        let mut diff = Vec::new();
        for (new, old) in new_tips.iter().zip(&old_tips) {
            let mut between = self.hashes_between(new, old)?;
            between.reverse();
            diff.extend(between);
        }
        Ok(diff)
    }
}
