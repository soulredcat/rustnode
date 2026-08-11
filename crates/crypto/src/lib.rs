#![forbid(unsafe_code)]
//! Minimal Alephium-compat cryptographic helpers for protocol handshake parity.

use blake2::{
    Blake2bVar,
    digest::{Update, VariableOutput},
};
use secp256k1::{Message, PublicKey as SecpPublicKey, Secp256k1, SecretKey};
use secp256k1::ecdsa::Signature as SecpSignature;

pub const HASH_LENGTH: usize = 32;
pub const PRIVATE_KEY_LENGTH: usize = 32;
pub const PUBLIC_KEY_LENGTH: usize = 33;
pub const SIGNATURE_LENGTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    InvalidLength {
        kind: &'static str,
        expected: usize,
        actual: usize,
    },
    InvalidPublicKey,
    InvalidPrivateKey,
    InvalidSignature,
}

impl core::fmt::Display for CryptoError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidLength {
                kind,
                expected,
                actual,
            } => {
                write!(
                    formatter,
                    "{kind} must be {expected} bytes (got {actual})"
                )
            }
            Self::InvalidPublicKey => formatter.write_str("invalid compressed secp256k1 public key"),
            Self::InvalidPrivateKey => formatter.write_str("invalid secp256k1 private key"),
            Self::InvalidSignature => formatter.write_str("invalid secp256k1 signature"),
        }
    }
}

impl std::error::Error for CryptoError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blake2bHash(pub [u8; HASH_LENGTH]);

impl Blake2bHash {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; HASH_LENGTH] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrivateKey([u8; PRIVATE_KEY_LENGTH]);

impl PrivateKey {
    #[must_use]
    pub const fn bytes(self) -> [u8; PRIVATE_KEY_LENGTH] {
        self.0
    }
}

impl TryFrom<&[u8]> for PrivateKey {
    type Error = CryptoError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != PRIVATE_KEY_LENGTH {
            return Err(CryptoError::InvalidLength {
                kind: "private key",
                expected: PRIVATE_KEY_LENGTH,
                actual: value.len(),
            });
        }

        let mut bytes = [0u8; PRIVATE_KEY_LENGTH];
        bytes.copy_from_slice(value);
        Ok(Self(bytes))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicKey([u8; PUBLIC_KEY_LENGTH]);

impl PublicKey {
    #[must_use]
    pub const fn bytes(self) -> [u8; PUBLIC_KEY_LENGTH] {
        self.0
    }
}

impl TryFrom<&[u8]> for PublicKey {
    type Error = CryptoError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != PUBLIC_KEY_LENGTH {
            return Err(CryptoError::InvalidLength {
                kind: "public key",
                expected: PUBLIC_KEY_LENGTH,
                actual: value.len(),
            });
        }

        if SecpPublicKey::from_slice(value).is_err() {
            return Err(CryptoError::InvalidPublicKey);
        }

        let mut bytes = [0u8; PUBLIC_KEY_LENGTH];
        bytes.copy_from_slice(value);
        Ok(Self(bytes))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature([u8; SIGNATURE_LENGTH]);

impl Signature {
    #[must_use]
    pub const fn bytes(self) -> [u8; SIGNATURE_LENGTH] {
        self.0
    }
}

impl TryFrom<&[u8]> for Signature {
    type Error = CryptoError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != SIGNATURE_LENGTH {
            return Err(CryptoError::InvalidLength {
                kind: "signature",
                expected: SIGNATURE_LENGTH,
                actual: value.len(),
            });
        }

        if SecpSignature::from_compact(value).is_err() {
            return Err(CryptoError::InvalidSignature);
        }

        let mut bytes = [0u8; SIGNATURE_LENGTH];
        bytes.copy_from_slice(value);
        Ok(Self(bytes))
    }
}

pub fn hash_blake2b_256(bytes: &[u8]) -> [u8; HASH_LENGTH] {
    let mut out = [0u8; HASH_LENGTH];
    let mut hasher = Blake2bVar::new(HASH_LENGTH).expect("fixed 32-byte output");
    hasher.update(bytes);
    hasher
        .finalize_variable(&mut out)
        .expect("hash output size must match requested bytes");
    out
}

pub fn hash_blake3(bytes: &[u8]) -> [u8; HASH_LENGTH] {
    let mut out = [0u8; HASH_LENGTH];
    out.copy_from_slice(blake3::hash(bytes).as_bytes());
    out
}

pub fn hash_blake3_double(bytes: &[u8]) -> [u8; HASH_LENGTH] {
    hash_blake3(&hash_blake3(bytes))
}

pub fn hash_blake2b(bytes: &[u8]) -> Blake2bHash {
    Blake2bHash(hash_blake2b_256(bytes))
}

pub fn sign(message: &[u8], private_key: &PrivateKey) -> Result<Signature, CryptoError> {
    if message.len() != HASH_LENGTH {
        return Err(CryptoError::InvalidLength {
            kind: "signature message",
            expected: HASH_LENGTH,
            actual: message.len(),
        });
    }

    let mut key = [0u8; PRIVATE_KEY_LENGTH];
    key.copy_from_slice(&private_key.0);
    let secp_secret = SecretKey::from_slice(&key).map_err(|_| CryptoError::InvalidPrivateKey)?;

    let secp = Secp256k1::new();
    let message = Message::from_digest_slice(message).map_err(|_| CryptoError::InvalidLength {
        kind: "signature message",
        expected: HASH_LENGTH,
        actual: message.len(),
    })?;
    let mut signature = secp.sign_ecdsa(&message, &secp_secret);
    signature.normalize_s();
    let compact = signature.serialize_compact();

    let mut output = [0u8; SIGNATURE_LENGTH];
    output.copy_from_slice(&compact);
    Ok(Signature(output))
}

pub fn verify(message: &[u8], signature: &Signature, public_key: &PublicKey) -> bool {
    if message.len() != HASH_LENGTH {
        return false;
    }

    let secp = Secp256k1::verification_only();
    let message = match Message::from_digest_slice(message) {
        Ok(message) => message,
        Err(_) => return false,
    };

    let key = match SecpPublicKey::from_slice(&public_key.0) {
        Ok(key) => key,
        Err(_) => return false,
    };

    let signature = match SecpSignature::from_compact(&signature.0) {
        Ok(signature) => signature,
        Err(_) => return false,
    };

    let mut normalized_signature = signature;
    normalized_signature.normalize_s();
    let signature = normalized_signature;

    secp.verify_ecdsa(&message, &signature, &key).is_ok()
}

#[cfg(test)]
mod tests {
    use super::{hash_blake2b_256, hash_blake3, hash_blake3_double};

    #[test]
    fn blake3_double_hash_matches_composition() {
        let bytes = b"alephium-header";
        let first = hash_blake3(bytes);
        assert_eq!(hash_blake3_double(bytes), hash_blake3(&first));
        assert_ne!(hash_blake2b_256(bytes), first);
    }
}
