//! Network constants (`network_{mainnet,testnet,devnet}.conf.tmpl`).

use crate::codec::TimeStamp;

pub const GROUPS: u32 = 4;
pub const MAINNET_ID: u8 = 0;
pub const TESTNET_ID: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HardFork {
    Mainnet,
    Leman,
    Rhone,
    Danube,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Network {
    pub id: u8,
    pub leman: TimeStamp,
    pub rhone: TimeStamp,
    pub danube: TimeStamp,
}

impl Network {
    /// Mainnet and testnet use their own schedules; any other id uses the
    /// devnet template's.
    #[must_use]
    pub const fn from_id(id: u8) -> Self {
        match id {
            MAINNET_ID => Self {
                id,
                leman: TimeStamp(1_680_170_400_000),
                rhone: TimeStamp(1_718_186_400_000),
                danube: TimeStamp(1_752_573_600_000),
            },
            TESTNET_ID => Self {
                id,
                leman: TimeStamp(1_676_210_400_000),
                rhone: TimeStamp(1_715_428_800_000),
                danube: TimeStamp(1_746_446_400_000),
            },
            _ => Self {
                id,
                leman: TimeStamp(1_643_500_800_000),
                rhone: TimeStamp(1_695_571_200_000),
                danube: TimeStamp(1_732_666_200_000),
            },
        }
    }

    /// `NetworkConfig.getHardFork`.
    #[must_use]
    pub fn hard_fork(&self, timestamp: TimeStamp) -> HardFork {
        if timestamp >= self.danube {
            HardFork::Danube
        } else if timestamp >= self.rhone {
            HardFork::Rhone
        } else if timestamp >= self.leman {
            HardFork::Leman
        } else {
            HardFork::Mainnet
        }
    }
}
