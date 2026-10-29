//! Header validation of new blocks (`HeaderValidation`), with the Danube
//! difficulty adjustment it checks the target against
//! (`FlowDifficultyAdjustment.getNextHashTargetDanubeUnsafe`,
//! `ChainDifficultyAdjustment`, `Target`, `Difficulty`).
//!
//! Every block rustnode validates is past Danube and past the penalty patch
//! (2026-01-14), so no older rule is implemented. The state hash check waits
//! for world state.

use std::time::{SystemTime, UNIX_EPOCH};

use alephium_model::network::MIN_HASH_ZERO_BITS;
use alephium_model::{BlockHeader, Hash, Target};
use num_bigint::BigUint;
use num_traits::{One, ToPrimitive, Zero};

use crate::{AddError, ChainStore, GENESIS_TIMESTAMP};

/// `ConsensusSetting.powAveragingWindow`.
const POW_AVERAGING_WINDOW: u32 = 17;
/// Danube `expectedWindowTimeSpan`: 8 s block target time times the window.
const EXPECTED_WINDOW_MILLIS: i64 = 8_000 * POW_AVERAGING_WINDOW as i64;
/// `windowTimeSpanMin` (`diffAdjustDownMax` 16 %).
const WINDOW_MIN_MILLIS: i64 = EXPECTED_WINDOW_MILLIS * (100 - 16) / 100;
/// `windowTimeSpanMax` (`diffAdjustUpMax` 8 %).
const WINDOW_MAX_MILLIS: i64 = EXPECTED_WINDOW_MILLIS * (100 + 8) / 100;
/// Danube `uncle-dependency-gap-time`.
const UNCLE_DEPENDENCY_GAP_MILLIS: u64 = 4_000;
/// `ConsensusConfigs.maxHeaderTimeStampDrift`.
const MAX_HEADER_TIMESTAMP_DRIFT_MILLIS: u64 = 15_000;
/// `ALPH.LaunchTimestamp`.
const LAUNCH_TIMESTAMP_MILLIS: u64 = 1_636_379_973_000;
/// `DefaultBlockVersion`.
const BLOCK_VERSION: u8 = 0;

/// `ScalingFactor`, `LowerGammaBound` (1 - 0.25), `UpperGammaBound` (1 + 0.5).
const SCALING: u32 = 1_000_000;
const LOWER_GAMMA: u32 = 750_000;
const UPPER_GAMMA: u32 = 1_500_000;

fn two_to_256() -> BigUint {
    BigUint::one() << 256u32
}

/// `ConsensusSetting.maxMiningTarget`: `2^(256 - 37) - 1`.
fn max_mining_target() -> BigUint {
    (BigUint::one() << (256 - MIN_HASH_ZERO_BITS)) - 1u32
}

/// `Target.fromCompactBitsUnsafe`.
pub(crate) fn target_value(target: &Target) -> BigUint {
    let size = u32::from(target.0[0]);
    let mantissa = BigUint::from(u32::from_be_bytes([
        0,
        target.0[1],
        target.0[2],
        target.0[3],
    ]));
    if size <= 3 {
        mantissa >> (8 * (3 - size))
    } else {
        mantissa << (8 * (size - 3))
    }
}

/// `Target.toCompactBitsUnsafe`: lossy, like the official encoding.
pub(crate) fn compact_target(value: &BigUint) -> Target {
    let size = u32::try_from(value.bits().div_ceil(8)).unwrap_or(u32::MAX);
    let mantissa = if size <= 3 {
        value.to_u32().unwrap_or(0) << (8 * (3 - size))
    } else {
        (value >> (8 * (size - 3))).to_u32().unwrap_or(0)
    };
    let bytes = mantissa.to_be_bytes();
    Target([
        u8::try_from(size).unwrap_or(u8::MAX),
        bytes[1],
        bytes[2],
        bytes[3],
    ])
}

fn invalid(reason: &str) -> AddError {
    AddError::Invalid(reason.to_owned())
}

/// `Target.getDifficulty`.
fn difficulty(target: &Target) -> Result<BigUint, AddError> {
    let value = target_value(target);
    if value.is_zero() {
        return Err(invalid("zero target"));
    }
    Ok(two_to_256() / value)
}

/// `Difficulty.getTarget`.
fn difficulty_target(difficulty: &BigUint) -> Result<Target, AddError> {
    if difficulty.is_zero() {
        Err(invalid("zero difficulty"))
    } else if difficulty.is_one() {
        Ok(compact_target(&(two_to_256() - 1u32)))
    } else {
        Ok(compact_target(&(two_to_256() / difficulty)))
    }
}

/// `ChainDifficultyAdjustment.getNextHashTarget`: capped at the easiest
/// target, compared as the official `maxMiningTarget` holds it (compact bits).
fn next_hash_target(value: &BigUint) -> Target {
    let max = compact_target(&max_mining_target());
    if *value <= target_value(&max) {
        compact_target(value)
    } else {
        max
    }
}

/// `ChainDifficultyAdjustment.reTarget`.
fn re_target(current: &Target, time_span_millis: i64) -> Result<Target, AddError> {
    let time_span = u64::try_from(time_span_millis).map_err(|_| invalid("negative time span"))?;
    let expected = u64::try_from(EXPECTED_WINDOW_MILLIS).unwrap_or(1);
    Ok(next_hash_target(
        &(target_value(current) * time_span / expected),
    ))
}

/// `ChainDifficultyAdjustment.calNextHashTargetRaw`: DigiShield, a quarter of
/// the deviation, clipped.
fn cal_next_hash_target_raw(current: &Target, window_time_span: i64) -> Result<Target, AddError> {
    let clipped = (EXPECTED_WINDOW_MILLIS + (window_time_span - EXPECTED_WINDOW_MILLIS) / 4)
        .clamp(WINDOW_MIN_MILLIS, WINDOW_MAX_MILLIS);
    re_target(current, clipped)
}

/// `FlowDifficultyAdjustment.getAdjustedDiff`.
fn adjusted_diff(average: &BigUint, chain: &BigUint) -> Result<BigUint, AddError> {
    if average.is_zero() {
        return Err(invalid("zero average difficulty"));
    }
    let gamma = chain * SCALING / average;
    let (lower, upper) = (BigUint::from(LOWER_GAMMA), BigUint::from(UPPER_GAMMA));
    if gamma >= lower && gamma <= upper {
        Ok(average.clone())
    } else if gamma > upper {
        Ok((average * &upper + (&gamma - &upper) * 11u32 * chain / 10u32) / &gamma)
    } else {
        Ok((&gamma * average + (&lower - &gamma) * 11u32 * chain / 10u32) / &lower)
    }
}

/// `FlowDifficultyAdjustment.clip`: within -2 % and +4 % of the parent's.
fn clip(parent: &BigUint, next: &BigUint) -> BigUint {
    let lower = parent * 98u32 / 100u32;
    let upper = parent * 104u32 / 100u32;
    if *next < lower {
        lower
    } else if *next > upper {
        upper
    } else {
        next.clone()
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

impl ChainStore {
    /// `HeaderValidation.checkHeader` for a block whose dependencies are all
    /// known, except the state hash.
    pub(crate) fn check_header(
        &self,
        hash: &Hash,
        header: &BlockHeader,
        from: u32,
        to: u32,
    ) -> Result<(), AddError> {
        if header.version != BLOCK_VERSION {
            return Err(invalid("block version"));
        }
        let parent = self.entry(&header.deps[(self.groups - 1 + to) as usize])?;
        let timestamp = header.timestamp.0;
        if timestamp < LAUNCH_TIMESTAMP_MILLIS || timestamp <= parent.block.header.timestamp.0 {
            return Err(invalid("timestamp not increasing"));
        }
        if now_millis() + MAX_HEADER_TIMESTAMP_DRIFT_MILLIS <= timestamp {
            return Err(invalid("timestamp too far ahead"));
        }
        self.check_deps_index(header, from)?;
        if !BlockHeader::check_work(hash, &header.target) {
            return Err(AddError::InvalidPow);
        }
        if self.next_target(from, to, &header.deps)? != header.target {
            return Err(invalid("work target"));
        }
        for dep in &header.deps {
            if BlockHeader::chain_index(dep, self.groups) != (from, to)
                && self.entry(dep)?.block.header.timestamp.0
                    > timestamp.saturating_sub(UNCLE_DEPENDENCY_GAP_MILLIS)
            {
                return Err(invalid("uncle dependency timestamp"));
            }
        }
        if !self.check_flow(&header.deps, from)? {
            return Err(invalid("header flow"));
        }
        Ok(())
    }

    /// `checkDepsIndex`: the other groups' intra-group tips, then this group's
    /// chain tips in order.
    fn check_deps_index(&self, header: &BlockHeader, from: u32) -> Result<(), AddError> {
        let groups = self.groups;
        let (in_deps, out_deps) = header.deps.split_at((groups - 1) as usize);
        let in_ok = (0..).zip(in_deps).all(|(index, dep)| {
            let group = if index < from { index } else { index + 1 };
            BlockHeader::chain_index(dep, groups) == (group, group)
        });
        let out_ok = (0..)
            .zip(out_deps)
            .all(|(index, dep)| BlockHeader::chain_index(dep, groups) == (from, index));
        if in_ok && out_ok {
            Ok(())
        } else {
            Err(invalid("deps index"))
        }
    }

    /// `getNextHashTargetDanubeUnsafe`: the target a block of `from -> to`
    /// with these dependencies must carry.
    pub(crate) fn next_target(
        &self,
        from: u32,
        to: u32,
        deps: &[Hash],
    ) -> Result<Target, AddError> {
        let chains = i64::from(self.groups * self.groups);
        let mut diff_sum = BigUint::zero();
        let mut time_span_sum = 0i64;
        let mut oldest = u64::MAX;
        for intra_dep in self.common_intra_group_deps(deps, from)? {
            let (diff, time_span, intra_oldest) = self.intra_dep_diff_and_time_span(&intra_dep)?;
            diff_sum += diff;
            time_span_sum += time_span;
            oldest = oldest.min(intra_oldest);
        }
        let average_diff = diff_sum / u64::try_from(chains).unwrap_or(1);
        let adjusted_average = difficulty(&cal_next_hash_target_raw(
            &difficulty_target(&average_diff)?,
            time_span_sum / chains,
        )?)?;

        let chain_dep = deps[(self.groups - 1 + to) as usize];
        let (diff, time_span) = self.diff_and_time_span(&chain_dep)?;
        let chain_diff = difficulty(&re_target(&difficulty_target(&diff)?, time_span)?)?;
        let adjusted = adjusted_diff(&adjusted_average, &chain_diff)?;

        let penalized = if self.height_gap(&chain_dep, oldest)? > POW_AVERAGING_WINDOW {
            adjusted * 105u32 / 100u32
        } else {
            adjusted
        };
        let next = difficulty_target(&clip(&diff, &penalized))?;
        Ok(next_hash_target(&target_value(&next)))
    }

    /// `calCommonIntraGroupDepsUnsafe`: per group, the lowest intra-group tip
    /// any of the block's intra-group dependencies sees.
    fn common_intra_group_deps(
        &self,
        deps: &[Hash],
        main_group: u32,
    ) -> Result<Vec<Hash>, AddError> {
        let groups = self.groups;
        let mut headers = Vec::with_capacity(groups as usize);
        for dep in deps[..(groups - 1) as usize]
            .iter()
            .chain([&deps[(groups - 1 + main_group) as usize]])
        {
            headers.push(self.entry(dep)?);
        }
        let mut common = Vec::with_capacity(groups as usize);
        for group in 0..groups {
            let mut best: Option<(Hash, u32)> = None;
            for entry in &headers {
                let header = &entry.block.header;
                let candidate = if header.timestamp == GENESIS_TIMESTAMP {
                    (self.genesis[self.chain(group, group)], 0)
                } else {
                    let intra_dep = match group.cmp(&entry.from) {
                        std::cmp::Ordering::Less => header.deps[group as usize],
                        std::cmp::Ordering::Greater => header.deps[group as usize - 1],
                        std::cmp::Ordering::Equal => header.deps[(groups - 1 + group) as usize],
                    };
                    (intra_dep, self.entry(&intra_dep)?.height)
                };
                if best.is_none_or(|(_, height)| candidate.1 < height) {
                    best = Some(candidate);
                }
            }
            common.push(best.map_or(Hash::ZERO, |(hash, _)| hash));
        }
        Ok(common)
    }

    /// `getDiffAndTimeSpanUnsafe(hash)`: the block's difficulty and the time
    /// its chain took for the last averaging window.
    fn diff_and_time_span(&self, hash: &Hash) -> Result<(BigUint, i64), AddError> {
        if *hash == Hash::ZERO {
            return Ok((
                difficulty(&compact_target(&max_mining_target()))?,
                EXPECTED_WINDOW_MILLIS,
            ));
        }
        let entry = self.entry(hash)?;
        let diff = difficulty(&entry.block.header.target)?;
        if entry.height < POW_AVERAGING_WINDOW + 1 {
            return Ok((diff, EXPECTED_WINDOW_MILLIS));
        }
        // `calTimeSpan`: back to the block at `height - powAveragingWindow`.
        let mut current = entry.clone();
        for _ in 0..POW_AVERAGING_WINDOW {
            current =
                self.entry(&current.block.header.deps[(self.groups - 1 + current.to) as usize])?;
        }
        let span = entry.block.header.timestamp.0 as i64 - current.block.header.timestamp.0 as i64;
        Ok((diff, span))
    }

    /// `getDiffAndTimeSpanForIntraDepUnsafe`: summed over the chain tips the
    /// intra-group block sees, with the oldest of their timestamps.
    fn intra_dep_diff_and_time_span(
        &self,
        intra_dep: &Hash,
    ) -> Result<(BigUint, i64, u64), AddError> {
        if *intra_dep == Hash::ZERO {
            let (diff, span) = self.diff_and_time_span(&Hash::ZERO)?;
            return Ok((
                diff * self.groups,
                span * i64::from(self.groups),
                GENESIS_TIMESTAMP.0,
            ));
        }
        let mut diff_sum = BigUint::zero();
        let mut time_span_sum = 0i64;
        let mut oldest = u64::MAX;
        for dep in self.out_tips(intra_dep)? {
            let (diff, time_span) = self.diff_and_time_span(&dep)?;
            diff_sum += diff;
            time_span_sum += time_span;
            oldest = oldest.min(self.entry(&dep)?.block.header.timestamp.0);
        }
        Ok((diff_sum, time_span_sum, oldest))
    }

    /// `calHeightDiffUnsafe`: how many blocks of the chain are newer than
    /// `oldest`.
    fn height_gap(&self, chain_dep: &Hash, oldest: u64) -> Result<u32, AddError> {
        let mut current = self.entry(chain_dep)?;
        let mut gap = 0;
        while current.block.header.timestamp.0 > oldest {
            gap += 1;
            current =
                self.entry(&current.block.header.deps[(self.groups - 1 + current.to) as usize])?;
        }
        Ok(gap)
    }

    /// `checkFlowDepsUnsafe`: every dependency's view of the flow is extended
    /// by the block's own dependencies.
    fn check_flow(&self, deps: &[Hash], from: u32) -> Result<bool, AddError> {
        let (in_deps, out_deps) = deps.split_at((self.groups - 1) as usize);
        for dep in deps {
            // getFlowTipsUnsafe(dep, from)
            let in_tips = (0..self.groups - 1)
                .map(|i| self.group_tip(dep, if i < from { i } else { i + 1 }))
                .collect::<Result<Vec<_>, _>>()?;
            let out_tips = self.out_tips(&self.group_tip(dep, from)?)?;
            for (block_tip, dep_tip) in in_deps.iter().zip(&in_tips) {
                if !self.is_extending(block_tip, dep_tip)? {
                    return Ok(false);
                }
            }
            for (block_tip, dep_tip) in out_deps.iter().zip(&out_tips) {
                if !self.is_extending(block_tip, dep_tip)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// `isExtendingUnsafe`: `current` is `previous` or a descendant of it, on
    /// `previous`'s chain.
    fn is_extending(&self, current: &Hash, previous: &Hash) -> Result<bool, AddError> {
        let (current_from, current_to) = BlockHeader::chain_index(current, self.groups);
        let (previous_from, previous_to) = BlockHeader::chain_index(previous, self.groups);
        if current_from != previous_from {
            return Ok(false);
        }
        if current_to == previous_to {
            self.is_before(previous, current)
        } else {
            let group_deps = self.out_tips(current)?;
            self.is_before(previous, &group_deps[previous_to as usize])
        }
    }

    /// `BlockHashChain.isBefore`.
    fn is_before(&self, earlier: &Hash, later: &Hash) -> Result<bool, AddError> {
        let earlier_entry = self.entry(earlier)?;
        let mut current = self.entry(later)?;
        if earlier_entry.height > current.height {
            return Ok(false);
        }
        if earlier_entry.height == current.height {
            return Ok(earlier == later);
        }
        let chain = self.chain(earlier_entry.from, earlier_entry.to);
        if self.chain_hashes(chain, earlier_entry.height).len() == 1 {
            return Ok(true);
        }
        while current.height > earlier_entry.height {
            current =
                self.entry(&current.block.header.deps[(self.groups - 1 + current.to) as usize])?;
        }
        Ok(current.block.hash() == *earlier)
    }
}

#[cfg(test)]
mod tests {
    use alephium_model::Target;
    use num_bigint::BigUint;

    use super::{adjusted_diff, clip, compact_target, max_mining_target, target_value};

    #[test]
    fn compact_targets_round_trip_like_the_official_encoding() {
        // 2^219 - 1: 28 bytes, mantissa 0x07ffff.
        assert_eq!(
            compact_target(&max_mining_target()),
            Target([0x1c, 0x07, 0xff, 0xff])
        );
        for bits in [
            [0x1b, 0x01, 0x23, 0x45],
            [0x03, 0x12, 0x34, 0x56],
            [0x01, 0x12, 0, 0],
        ] {
            assert_eq!(compact_target(&target_value(&Target(bits))), Target(bits));
        }
        // Lossy: the low bytes below the 3-byte mantissa are dropped.
        let value = (BigUint::from(0x0123_4567u32) << 200u32) + 1u32;
        assert_eq!(
            target_value(&compact_target(&value)),
            BigUint::from(0x01_2345u32) << 208u32
        );
    }

    #[test]
    fn adjusted_difficulty_and_clip_follow_the_official_formulas() {
        let average = BigUint::from(1_000_000u32);
        // Within [0.75, 1.5] the average is kept.
        assert_eq!(
            adjusted_diff(&average, &BigUint::from(1_200_000u32)),
            Ok(average.clone())
        );
        // gamma = 2.0: (1.5 * avg + 0.5 * 1.1 * 2 * avg) / 2.0 = 1.3 * avg.
        assert_eq!(
            adjusted_diff(&average, &BigUint::from(2_000_000u32)),
            Ok(BigUint::from(1_300_000u32))
        );
        // gamma = 0.5: (0.5 * avg + 0.25 * 1.1 * 0.5 * avg) / 0.75.
        assert_eq!(
            adjusted_diff(&average, &BigUint::from(500_000u32)),
            Ok(BigUint::from(850_000u32))
        );
        let parent = BigUint::from(1000u32);
        assert_eq!(clip(&parent, &BigUint::from(900u32)), BigUint::from(980u32));
        assert_eq!(
            clip(&parent, &BigUint::from(1100u32)),
            BigUint::from(1040u32)
        );
        assert_eq!(
            clip(&parent, &BigUint::from(1010u32)),
            BigUint::from(1010u32)
        );
    }
}
