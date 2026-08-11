#![forbid(unsafe_code)]
//! Utility primitives shared by the Rust rebuild.

/// Official Alephium compatibility baseline.
pub const UPSTREAM_BASELINE_COMMIT: &str = "63bcf6998508025f9538f3377629fe9e66df6c53";

pub const TIMESTAMP_BYTE_LENGTH: usize = core::mem::size_of::<i64>();
pub const U32_BYTE_LENGTH: usize = core::mem::size_of::<u32>();

/// M1-compatible canonical timestamp wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeStamp(i64);

impl TimeStamp {
    /// Valid canonical timestamp in the protocol is non-negative.
    pub const ZERO: TimeStamp = TimeStamp(0);

    pub const fn new(millis: i64) -> Option<Self> {
        if millis >= 0 {
            Some(Self(millis))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn millis(self) -> i64 {
        self.0
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl core::fmt::Display for TimeStamp {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Unsigned 32-bit value used by protocol counters and request identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct U32(pub u32);

impl U32 {
    pub const ZERO: U32 = U32(0);

    pub const fn get(self) -> u32 {
        self.0
    }
}
