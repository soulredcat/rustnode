//! The best view of the block flow in Danube: the dependencies a node builds
//! on and keeps its mempool against (`BlockFlow.calBestFlowSkeletonUnsafe`,
//! `calBestFlowPerChainIndexUnsafe`, `FlowTipsUtil.tryMergeUnsafe`,
//! `FlowUtils.calMemPoolChangesUnsafe`). Danube merges tips without the
//! conflict check of earlier forks, so the view needs no state.

use std::cmp::Ordering;

use alephium_model::{BlockHeader, Hash, U256};

use crate::{AddError, ChainStore, compare_hashes};

/// `maxSyncBlocksPerChain`: a view that jumps further ahead than this leaves
/// the mempool untouched (`FlowUtils.updateGrandPoolUnsafe`).
const MAX_MEMPOOL_HEIGHT_GAP: u32 = 50;

/// `BlockHashPool.compareWeight`: weight, then hash bytes.
fn compare_weight(hash0: &Hash, weight0: U256, hash1: &Hash, weight1: U256) -> Ordering {
    weight0
        .cmp(&weight1)
        .then_with(|| compare_hashes(hash0, hash1))
}

/// `BlockFlowSkeleton`: the tip of every intra-group chain and its out tips.
struct Skeleton {
    tips: Vec<Hash>,
    out_tips: Vec<Vec<Hash>>,
}

/// What the mempool of a group does when its view moves
/// (`MemPoolChanges`): blocks that entered the view, whose transactions are
/// now confirmed, and blocks that left it, whose transactions return.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ViewChange {
    pub entered: Vec<Hash>,
    pub left: Vec<Hash>,
}

impl ChainStore {
    fn weight_of(&self, hash: &Hash) -> U256 {
        self.get(hash)
            .and_then(|entry| entry.weight)
            .unwrap_or(U256::ZERO)
    }

    /// A chain's tips, heaviest first (`sorted(blockHashOrdering.reverse)`).
    fn tips_heaviest_first(&self, from: u32, to: u32) -> Vec<Hash> {
        let mut tips: Vec<(Hash, U256)> = self.tips[self.chain(from, to)]
            .keys()
            .map(|tip| (*tip, self.weight_of(tip)))
            .collect();
        tips.sort_by(|(hash0, weight0), (hash1, weight1)| {
            compare_weight(hash1, *weight1, hash0, *weight0)
        });
        tips.into_iter().map(|(hash, _)| hash).collect()
    }

    /// `getBestIntraGroupTip`: the heaviest tip of all intra-group chains.
    fn best_intra_group_tip(&self) -> Hash {
        (0..self.groups)
            .filter_map(|group| self.tips_heaviest_first(group, group).first().copied())
            .max_by(|hash0, hash1| {
                compare_weight(hash0, self.weight_of(hash0), hash1, self.weight_of(hash1))
            })
            .unwrap_or(self.genesis[0])
    }

    /// `calBestFlowSkeletonUnsafe`.
    fn best_skeleton(&self) -> Result<Skeleton, AddError> {
        let best = self.best_intra_group_tip();
        let (best_group, _) = BlockHeader::chain_index(&best, self.groups);
        let mut tips = Vec::with_capacity(self.groups as usize);
        let mut out_tips = Vec::with_capacity(self.groups as usize);
        for group in 0..self.groups {
            let tip = self.group_tip(&best, group)?;
            out_tips.push(self.out_tips(&tip)?);
            tips.push(tip);
        }
        let mut skeleton = Skeleton { tips, out_tips };
        for group in (0..self.groups).filter(|group| *group != best_group) {
            for tip in self.tips_heaviest_first(group, group) {
                if let Some(extended) = self.try_extend_skeleton(&skeleton, group, &tip)? {
                    skeleton = extended;
                    break;
                }
            }
        }
        Ok(skeleton)
    }

    /// `tryExtendBlockFlowSkeletonUnsafe`.
    fn try_extend_skeleton(
        &self,
        skeleton: &Skeleton,
        group: u32,
        tip: &Hash,
    ) -> Result<Option<Skeleton>, AddError> {
        let current = skeleton.tips[group as usize];
        if *tip == current || !self.is_extending(tip, &current)? {
            return Ok(None);
        }
        let mut tips = skeleton.tips.clone();
        let mut out_tips = skeleton.out_tips.clone();
        tips[group as usize] = *tip;
        out_tips[group as usize] = self.out_tips(tip)?;
        for other in (0..self.groups).filter(|other| *other != group) {
            let of_tip = self.group_tip(tip, other)?;
            let of_skeleton = skeleton.tips[other as usize];
            if self.is_extending(&of_skeleton, &of_tip)? {
                continue;
            } else if self.is_extending(&of_tip, &of_skeleton)? {
                tips[other as usize] = of_tip;
                out_tips[other as usize] = self.out_tips(&of_tip)?;
            } else {
                return Ok(None);
            }
        }
        Ok(Some(Skeleton { tips, out_tips }))
    }

    /// `FlowTipsUtil.merge` over two tip lists, position by position.
    fn merge_tips(&self, tips0: &[Hash], tips1: &[Hash]) -> Result<Option<Vec<Hash>>, AddError> {
        let mut merged = Vec::with_capacity(tips0.len());
        for (tip0, tip1) in tips0.iter().zip(tips1) {
            if self.is_extending(tip0, tip1)? {
                merged.push(*tip0);
            } else if self.is_extending(tip1, tip0)? {
                merged.push(*tip1);
            } else {
                return Ok(None);
            }
        }
        Ok(Some(merged))
    }

    /// `extendFlowFromChainUnsafe`: the heaviest tip of `from -> to` that
    /// extends the view and merges into it.
    fn extend_from_chain(
        &self,
        in_tips: &mut Vec<Hash>,
        out_tips: &mut Vec<Hash>,
        from: u32,
        to: u32,
    ) -> Result<(), AddError> {
        let current = out_tips[to as usize];
        for tip in self.tips_heaviest_first(from, to) {
            if tip == current || !self.is_extending(&tip, &current)? {
                continue;
            }
            // getLightTipsUnsafe(tip, from): the tip's view of the other
            // groups, and the tip itself as the out tip.
            let light_in = (0..self.groups - 1)
                .map(|i| self.group_tip(&tip, if i < from { i } else { i + 1 }))
                .collect::<Result<Vec<_>, _>>()?;
            let Some(new_in) = self.merge_tips(in_tips, &light_in)? else {
                continue;
            };
            let new_out = if out_tips[to as usize] == tip {
                Some(out_tips.clone())
            } else {
                self.merge_tips(out_tips, &self.out_tips(&tip)?)?
            };
            if let Some(new_out) = new_out {
                *in_tips = new_in;
                *out_tips = new_out;
                return Ok(());
            }
        }
        Ok(())
    }

    /// `calBestFlowPerChainIndexUnsafe`: the dependencies of the best view of
    /// `from -> to` (in tips, then out tips).
    pub fn best_deps(&self, from: u32, to: u32) -> Result<Vec<Hash>, AddError> {
        let skeleton = self.best_skeleton()?;
        let mut in_tips: Vec<Hash> = (0..self.groups)
            .filter(|group| *group != from)
            .map(|group| skeleton.tips[group as usize])
            .collect();
        let mut out_tips = skeleton.out_tips[from as usize].clone();
        if from == to {
            for chain_to in 0..self.groups {
                self.extend_from_chain(&mut in_tips, &mut out_tips, from, chain_to)?;
            }
        } else {
            self.extend_from_chain(&mut in_tips, &mut out_tips, from, to)?;
        }
        in_tips.extend(out_tips);
        Ok(in_tips)
    }

    /// `BlockHashChain.calHashDiff`: blocks after the common ancestor up to
    /// `new` (entered) and up to `old` (left), on one chain.
    fn hash_diff(&self, new: &Hash, old: &Hash) -> Result<ViewChange, AddError> {
        let mut change = ViewChange::default();
        let (mut new_entry, mut old_entry) = (self.entry(new)?, self.entry(old)?);
        let (mut new_hash, mut old_hash) = (*new, *old);
        while new_hash != old_hash {
            if new_entry.height >= old_entry.height {
                change.entered.push(new_hash);
                new_hash = new_entry.block.header.deps[(self.groups - 1 + new_entry.to) as usize];
                new_entry = self.entry(&new_hash)?;
            } else {
                change.left.push(old_hash);
                old_hash = old_entry.block.header.deps[(self.groups - 1 + old_entry.to) as usize];
                old_entry = self.entry(&old_hash)?;
            }
        }
        Ok(change)
    }

    /// `calMemPoolChangesUnsafe`, behind the `updateGrandPoolUnsafe` height
    /// guard: how the mempool of `group` follows its view from `old_deps` to
    /// `new_deps`; `None` when the view jumped too far ahead to bother.
    pub fn view_change(
        &self,
        group: u32,
        old_deps: &[Hash],
        new_deps: &[Hash],
    ) -> Result<Option<ViewChange>, AddError> {
        let split = (self.groups - 1) as usize;
        let (old_in, old_out) = old_deps.split_at(split);
        let (new_in, new_out) = new_deps.split_at(split);
        let new_height = self.entry(&new_out[group as usize])?.height;
        let old_height = self.entry(&old_out[group as usize])?.height;
        if new_height > old_height + MAX_MEMPOOL_HEIGHT_GAP {
            return Ok(None);
        }
        let mut change = ViewChange::default();
        for (new_intra, old_intra) in new_in.iter().zip(old_in) {
            let new_tip = self.out_tips(new_intra)?[group as usize];
            let old_tip = self.out_tips(old_intra)?[group as usize];
            let diff = self.hash_diff(&new_tip, &old_tip)?;
            change.entered.extend(diff.entered);
            change.left.extend(diff.left);
        }
        for (new_tip, old_tip) in new_out.iter().zip(old_out) {
            let diff = self.hash_diff(new_tip, old_tip)?;
            change.entered.extend(diff.entered);
            change.left.extend(diff.left);
        }
        Ok(Some(change))
    }
}
