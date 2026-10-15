//! 32-byte hashes: Blake2b-256 (`Hash`) for transactions and state, double
//! Blake3 for block hashes (`BlockHash.doubleHash`).

use core::fmt;

use alephium_serde::Error;

use crate::codec::{Codec, Reader};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Hash(pub [u8; 32]);

impl Hash {
    pub const ZERO: Self = Self([0; 32]);

    #[must_use]
    pub fn blake2b(bytes: &[u8]) -> Self {
        Self(alephium_crypto::hash_blake2b_256(bytes))
    }

    /// `Blake3.doubleHash`: the block hash of serialized header bytes.
    #[must_use]
    pub fn blake3_double(bytes: &[u8]) -> Self {
        let first = blake3::hash(bytes);
        Self(*blake3::hash(first.as_bytes()).as_bytes())
    }

    #[must_use]
    pub fn to_hex(&self) -> String {
        hex(&self.0)
    }

    pub fn from_hex(text: &str) -> Option<Self> {
        let bytes = from_hex(text)?;
        let array: [u8; 32] = bytes.try_into().ok()?;
        Some(Self(array))
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl Codec for Hash {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self(reader.array()?))
    }
}

/// `DjbHash.intHash`.
#[must_use]
pub fn djb_hash(bytes: &[u8]) -> i32 {
    bytes.iter().fold(5381i32, |hash, byte| {
        hash.wrapping_shl(5)
            .wrapping_add(hash)
            .wrapping_add(i32::from(*byte))
    })
}

#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

#[must_use]
pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    text.as_bytes()
        .chunks(2)
        .map(|pair| {
            let digit = |c: u8| (c as char).to_digit(16);
            u8::try_from(digit(pair[0])? * 16 + digit(pair[1])?).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Hash, djb_hash, from_hex};

    #[test]
    fn hashes_and_hex() {
        assert_eq!(djb_hash(b""), 5381);
        assert_eq!(
            Hash::blake2b(b"").to_hex(),
            "0e5751c026e543b2e8ab2eb06099daa1d1e5df47778f7787faab45cdf12fe3a8"
        );
        // Blake3("") = af1349b9..., hashed once more.
        assert_eq!(
            Hash::blake3_double(b""),
            Hash(*blake3::hash(blake3::hash(b"").as_bytes()).as_bytes())
        );
        assert_eq!(from_hex("0aFf"), Some(vec![0x0a, 0xff]));
        assert_eq!(from_hex("0"), None);
        assert_eq!(from_hex("zz"), None);
    }
}
