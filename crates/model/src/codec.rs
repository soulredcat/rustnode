//! The official binary encoding (`org.alephium.serde`) for model types.
//!
//! Decoding follows the official rules exactly, including what it accepts:
//! multi-byte compact integers are decoded even when a shorter form exists, and
//! values are re-encoded canonically, as the official node does before hashing.

use alephium_serde::{Error, compact};
use ethnum::{I256, U256};

/// A cursor over untrusted input.
#[derive(Debug, Clone, Copy)]
pub struct Reader<'a> {
    input: &'a [u8],
}

impl<'a> Reader<'a> {
    #[must_use]
    pub const fn new(input: &'a [u8]) -> Self {
        Self { input }
    }

    #[must_use]
    pub const fn rest(&self) -> &'a [u8] {
        self.input
    }

    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.input.len()
    }

    pub fn take(&mut self, length: usize) -> Result<&'a [u8], Error> {
        if self.input.len() < length {
            return Err(Error::not_enough_bytes(length, self.input.len()));
        }
        let (taken, rest) = self.input.split_at(length);
        self.input = rest;
        Ok(taken)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let bytes = self.take(N)?;
        let mut array = [0u8; N];
        array.copy_from_slice(bytes);
        Ok(array)
    }

    pub fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    /// `Serde[Int]`: signed compact integer.
    pub fn int(&mut self) -> Result<i32, Error> {
        let staging = compact::decode_i32(self.input)?;
        self.input = staging.rest;
        Ok(staging.value)
    }

    /// A signed compact length that must be non-negative and cannot exceed the
    /// remaining input (every element takes at least one byte), so a hostile
    /// length never causes a large allocation.
    pub fn length(&mut self) -> Result<usize, Error> {
        let length = self.int()?;
        let length = usize::try_from(length)
            .map_err(|_| Error::validation(format!("Negative length: {length}")))?;
        if length > self.input.len() {
            return Err(Error::not_enough_bytes(length, self.input.len()));
        }
        Ok(length)
    }
}

pub trait Codec: Sized {
    fn encode(&self, out: &mut Vec<u8>);
    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error>;
}

#[must_use]
pub fn serialize<T: Codec>(value: &T) -> Vec<u8> {
    let mut out = Vec::new();
    value.encode(&mut out);
    out
}

/// Decodes exactly one value; trailing bytes are an error, as in the official
/// `deserialize`.
pub fn deserialize<T: Codec>(bytes: &[u8]) -> Result<T, Error> {
    let mut reader = Reader::new(bytes);
    let value = T::decode(&mut reader)?;
    if reader.remaining() == 0 {
        Ok(value)
    } else {
        Err(Error::redundant_bytes(
            bytes.len() - reader.remaining(),
            bytes.len(),
        ))
    }
}

impl Codec for u8 {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(*self);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        reader.byte()
    }
}

impl Codec for bool {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(u8::from(*self));
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        match reader.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            byte => Err(Error::validation(format!("Invalid bool from byte {byte}"))),
        }
    }
}

impl Codec for i32 {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&compact::encode_i32(*self));
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        reader.int()
    }
}

impl<const N: usize> Codec for [u8; N] {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        reader.array()
    }
}

/// `Serde[ByteString]`: signed compact length, then the bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Bytes(pub Vec<u8>);

pub(crate) fn encode_bytes(bytes: &[u8], out: &mut Vec<u8>) {
    encode_length(bytes.len(), out);
    out.extend_from_slice(bytes);
}

impl Codec for Bytes {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_bytes(&self.0, out);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        let length = reader.length()?;
        Ok(Self(reader.take(length)?.to_vec()))
    }
}

pub(crate) fn encode_length(length: usize, out: &mut Vec<u8>) {
    // Lengths come from decoded or locally built values, far below i32::MAX.
    let length = i32::try_from(length).unwrap_or(i32::MAX);
    out.extend_from_slice(&compact::encode_i32(length));
}

/// `Serde[AVector[T]]`.
impl<T: Codec> Codec for Vec<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_length(self.len(), out);
        for item in self {
            item.encode(out);
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        let length = reader.length()?;
        let mut items = Vec::with_capacity(length);
        for _ in 0..length {
            items.push(T::decode(reader)?);
        }
        Ok(items)
    }
}

/// `Serde[Option[T]]`: flag byte 0 (none) or 1 (some).
impl<T: Codec> Codec for Option<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            None => out.push(0),
            Some(value) => {
                out.push(1);
                value.encode(out);
            }
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        match reader.byte()? {
            0 => Ok(None),
            1 => Ok(Some(T::decode(reader)?)),
            _ => Err(Error::wrong_format("expect 0 or 1 for option flag")),
        }
    }
}

impl<A: Codec, B: Codec> Codec for (A, B) {
    fn encode(&self, out: &mut Vec<u8>) {
        self.0.encode(out);
        self.1.encode(out);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        Ok((A::decode(reader)?, B::decode(reader)?))
    }
}

const MULTI_BYTE_PREFIX: u8 = 0xc0;
const FOUR_BYTE_UNSIGNED_BOUND: u32 = 0x40 << 24;
const FOUR_BYTE_SIGNED_BOUND: i32 = 0x20 << 24;

/// Splits off one compact integer: its mode, its body (including the header
/// byte), as the official `CompactInteger.Mode.decode`.
fn compact_body<'a>(reader: &mut Reader<'a>) -> Result<(bool, &'a [u8]), Error> {
    let Some(&first) = reader.rest().first() else {
        return Err(Error::not_enough_bytes(1, 0));
    };
    let (multi, length) = match first & 0xc0 {
        0x00 => (false, 1),
        0x40 => (false, 2),
        0x80 => (false, 4),
        _ => (true, usize::from(first & 0x3f) + 5),
    };
    Ok((multi, reader.take(length)?))
}

/// `Serde[U256]`: unsigned compact integer.
impl Codec for U256 {
    fn encode(&self, out: &mut Vec<u8>) {
        if *self < U256::from(FOUR_BYTE_UNSIGNED_BOUND) {
            out.extend_from_slice(&compact::encode_u32(self.as_u32()));
        } else {
            let bytes = self.to_be_bytes();
            let start = bytes.iter().position(|byte| *byte != 0).unwrap_or(31);
            let data = &bytes[start..];
            // data is at least 4 bytes long because the value is >= 2^30.
            out.push(MULTI_BYTE_PREFIX + (data.len() as u8 - 4));
            out.extend_from_slice(data);
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        let mut peek = *reader;
        let (multi, body) = compact_body(&mut peek)?;
        if !multi {
            let value = reader.int_unsigned()?;
            return Ok(U256::from(value));
        }
        *reader = peek;
        let data = &body[1..];
        let start = data
            .iter()
            .position(|byte| *byte != 0)
            .unwrap_or(data.len());
        let significant = &data[start..];
        if significant.len() > 32 {
            return Err(Error::validation("Expect U256, but get a larger value"));
        }
        let mut bytes = [0u8; 32];
        bytes[32 - significant.len()..].copy_from_slice(significant);
        Ok(U256::from_be_bytes(bytes))
    }
}

impl Reader<'_> {
    fn int_unsigned(&mut self) -> Result<u32, Error> {
        let staging = compact::decode_u32(self.input)?;
        self.input = staging.rest;
        Ok(staging.value)
    }
}

/// `Serde[I256]`: signed compact integer.
impl Codec for I256 {
    fn encode(&self, out: &mut Vec<u8>) {
        let small = I256::from(-FOUR_BYTE_SIGNED_BOUND)..I256::from(FOUR_BYTE_SIGNED_BOUND);
        if small.contains(self) {
            out.extend_from_slice(&compact::encode_i32(self.as_i32()));
        } else {
            let bytes = self.to_be_bytes();
            let data = minimal_twos_complement(&bytes);
            out.push(MULTI_BYTE_PREFIX + (data.len() as u8 - 4));
            out.extend_from_slice(data);
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        let mut peek = *reader;
        let (multi, body) = compact_body(&mut peek)?;
        if !multi {
            return Ok(I256::from(reader.int()?));
        }
        *reader = peek;
        let data = &body[1..];
        if data.len() > 32 {
            return Err(Error::validation("Expect I256, but get a larger value"));
        }
        let fill = if data[0] & 0x80 == 0 { 0x00 } else { 0xff };
        let mut bytes = [fill; 32];
        bytes[32 - data.len()..].copy_from_slice(data);
        Ok(I256::from_be_bytes(bytes))
    }
}

/// Java `BigInteger.toByteArray`: the shortest two's-complement big-endian form.
fn minimal_twos_complement(bytes: &[u8; 32]) -> &[u8] {
    let mut start = 0;
    while start < 31 {
        let redundant = (bytes[start] == 0x00 && bytes[start + 1] & 0x80 == 0)
            || (bytes[start] == 0xff && bytes[start + 1] & 0x80 != 0);
        if !redundant {
            break;
        }
        start += 1;
    }
    &bytes[start..]
}

/// `TimeStamp`: milliseconds as an 8-byte big-endian non-negative long.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TimeStamp(pub u64);

impl Codec for TimeStamp {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0.to_be_bytes());
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        let millis = i64::from_be_bytes(reader.array()?);
        u64::try_from(millis)
            .map(Self)
            .map_err(|_| Error::validation("Negative timestamp"))
    }
}

#[cfg(test)]
mod tests {
    use ethnum::{I256, U256};

    use super::{Bytes, TimeStamp, deserialize, serialize};

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// Expected bytes printed by the official v4.7.2 jar
    /// (`CompactInteger.Unsigned.encode` / `Signed.encode`) at every mode boundary.
    #[test]
    fn u256_compact_encoding_matches_official_modes() {
        for (value, expected) in [
            (U256::ZERO, "00"),
            (U256::from(0x3fu32), "3f"),
            (U256::from(0x40u32), "4040"),
            (U256::from(0x3fffu32), "7fff"),
            (U256::from(0x4000u32), "80004000"),
            (U256::from(0x3fff_ffffu32), "bfffffff"),
            (U256::from(0x4000_0000u32), "c040000000"),
            (U256::from(u32::MAX), "c0ffffffff"),
            (U256::from(1u64 << 32), "c10100000000"),
            (U256::MAX, &format!("dc{}", "ff".repeat(32))),
        ] {
            let encoded = serialize(&value);
            assert_eq!(hex(&encoded), expected, "{value}");
            assert_eq!(deserialize::<U256>(&encoded), Ok(value));
        }
    }

    #[test]
    fn i256_compact_encoding_matches_official_modes() {
        for (value, expected) in [
            (I256::ZERO, "00"),
            (I256::from(-1), "3f"),
            (I256::from(0x1f), "1f"),
            (I256::from(-0x20), "20"),
            (I256::from(0x20), "4020"),
            (I256::from(0x1fff_ffff), "9fffffff"),
            (I256::from(-0x2000_0000), "a0000000"),
            (I256::from(0x2000_0000), "c020000000"),
            (I256::from(-0x2000_0001), "c0dfffffff"),
            (I256::from(i64::from(i32::MAX) + 1), "c10080000000"),
            (I256::MAX, &format!("dc7f{}", "ff".repeat(31))),
            (I256::MIN, &format!("dc80{}", "00".repeat(31))),
        ] {
            let encoded = serialize(&value);
            assert_eq!(hex(&encoded), expected, "{value}");
            assert_eq!(deserialize::<I256>(&encoded), Ok(value));
        }
    }

    #[test]
    fn non_canonical_multi_byte_values_decode_like_the_official_node() {
        // 5 in four-byte multi-byte form, with a leading zero byte.
        assert_eq!(
            deserialize::<U256>(&[0xc1, 0, 0, 0, 0, 5]),
            Ok(U256::from(5u8))
        );
        // 33 significant bytes do not fit.
        let mut too_big = vec![0xc0 + 29, 1];
        too_big.extend([0u8; 32]);
        assert!(deserialize::<U256>(&too_big).is_err());
    }

    #[test]
    fn bytes_vectors_options_and_timestamps() {
        let value = (
            Bytes(vec![1, 2, 3]),
            (vec![Some(true), None], TimeStamp(1_231_006_505_000)),
        );
        let encoded = serialize(&value);
        assert_eq!(hex(&encoded), "03010203020101000000011e9db49828");
        assert_eq!(
            deserialize::<(Bytes, (Vec<Option<bool>>, TimeStamp))>(&encoded),
            Ok(value)
        );
        assert!(deserialize::<TimeStamp>(&[0x80, 0, 0, 0, 0, 0, 0, 0]).is_err());
        assert!(deserialize::<bool>(&[2]).is_err());
        assert!(deserialize::<Vec<u8>>(&[0x05, 1, 2]).is_err());
        assert!(deserialize::<Vec<u8>>(&[0x3f]).is_err());
        assert!(deserialize::<u8>(&[1, 2]).is_err());
    }
}
