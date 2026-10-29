//! Mainnet constants (`network_mainnet.conf.tmpl`). rustnode serves mainnet
//! only (`IMMUTABLE.md`).

use crate::codec::TimeStamp;

/// Mainnet's network id.
pub const NETWORK_ID: u8 = 0;
/// `leman-hard-fork-timestamp`.
pub const LEMAN: TimeStamp = TimeStamp(1_680_170_400_000);
/// `rhone-hard-fork-timestamp`.
pub const RHONE: TimeStamp = TimeStamp(1_718_186_400_000);
/// `danube-hard-fork-timestamp`.
pub const DANUBE: TimeStamp = TimeStamp(1_752_573_600_000);
/// `consensus.num-zeros-at-least-in-hash`: the leading zero bits every valid
/// block hash has, i.e. the easiest target mainnet allows (`maxMiningTarget`).
pub const MIN_HASH_ZERO_BITS: u32 = 37;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HardFork {
    Mainnet,
    Leman,
    Rhone,
    Danube,
}

/// `NetworkConfig.getHardFork`.
#[must_use]
pub fn hard_fork(timestamp: TimeStamp) -> HardFork {
    if timestamp >= DANUBE {
        HardFork::Danube
    } else if timestamp >= RHONE {
        HardFork::Rhone
    } else if timestamp >= LEMAN {
        HardFork::Leman
    } else {
        HardFork::Mainnet
    }
}
