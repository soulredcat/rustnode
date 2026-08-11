use core::fmt;

/// Error returned while encoding or decoding official Alephium bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The input ended before the complete value was available.
    NotEnoughBytes { expected: usize, actual: usize },
    /// A complete value was followed by bytes where an exact decode was required.
    RedundantBytes { expected: usize, actual: usize },
    /// The bytes do not represent the requested Alephium type.
    WrongFormat { message: String },
    /// The bytes are structurally decodable but fail a type-level validation rule.
    Validation { message: String },
    /// An untrusted declared size exceeds the local bound for this decode path.
    LimitExceeded { declared: usize, limit: usize },
    /// A Rust collection cannot be represented by Alephium's signed length prefix.
    LengthOutOfRange { length: usize },
}

impl Error {
    #[must_use]
    pub const fn not_enough_bytes(expected: usize, actual: usize) -> Self {
        Self::NotEnoughBytes { expected, actual }
    }

    #[must_use]
    pub const fn redundant_bytes(expected: usize, actual: usize) -> Self {
        Self::RedundantBytes { expected, actual }
    }

    #[must_use]
    pub fn wrong_format(message: impl Into<String>) -> Self {
        Self::WrongFormat {
            message: message.into(),
        }
    }

    #[must_use]
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEnoughBytes { expected, actual } => {
                write!(
                    formatter,
                    "too few bytes: expected {expected}, got {actual}"
                )
            }
            Self::RedundantBytes { expected, actual } => {
                write!(
                    formatter,
                    "too many bytes: expected {expected}, got {actual}"
                )
            }
            Self::WrongFormat { message } | Self::Validation { message } => {
                formatter.write_str(message)
            }
            Self::LimitExceeded { declared, limit } => {
                write!(formatter, "declared size {declared} exceeds limit {limit}")
            }
            Self::LengthOutOfRange { length } => {
                write!(
                    formatter,
                    "length {length} cannot be represented as a signed 32-bit value"
                )
            }
        }
    }
}

impl std::error::Error for Error {}
