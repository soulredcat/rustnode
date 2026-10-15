//! Block-download payloads of P2P v2 sync and block announcements
//! (`protocol/message/Payload.scala`): `BlocksAndUnclesByHeightsRequest`,
//! `BlocksAndUnclesByHeightsResponse`, `HeadersByHeightsRequest`,
//! `HeadersByHeightsResponse`, and `NewBlock`.

use alephium_model::codec::Reader;
use alephium_model::{Block, BlockHeader, Codec};
use alephium_serde::{Error, compact};

/// `(ChainIndex, BlockHeightRange)`: from and to group as compact ints, then
/// `from`, `to`, `step` heights.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeightRange {
    pub from_group: i32,
    pub to_group: i32,
    pub from_height: i32,
    pub to_height: i32,
    pub step: i32,
}

impl HeightRange {
    /// `BlockHeightRange.isValid`.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.from_height >= 0 && self.to_height >= self.from_height && self.step >= 1
    }

    fn encode(&self, out: &mut Vec<u8>) {
        for value in [
            self.from_group,
            self.to_group,
            self.from_height,
            self.to_height,
            self.step,
        ] {
            out.extend_from_slice(&compact::encode_i32(value));
        }
    }
}

fn encode_request(id: u32, ranges: &[HeightRange]) -> Vec<u8> {
    let mut out = compact::encode_u32(id);
    let length = i32::try_from(ranges.len()).unwrap_or(i32::MAX);
    out.extend_from_slice(&compact::encode_i32(length));
    for range in ranges {
        range.encode(&mut out);
    }
    out
}

#[must_use]
pub fn encode_blocks_and_uncles_by_heights_request(id: u32, ranges: &[HeightRange]) -> Vec<u8> {
    encode_request(id, ranges)
}

#[must_use]
pub fn encode_headers_by_heights_request(id: u32, ranges: &[HeightRange]) -> Vec<u8> {
    encode_request(id, ranges)
}

fn decode_id(reader: &mut Reader<'_>) -> Result<u32, Error> {
    let staging = compact::decode_u32(reader.rest())?;
    let consumed = reader.remaining() - staging.rest.len();
    reader.take(consumed)?;
    Ok(staging.value)
}

fn finish<T>(reader: &Reader<'_>, value: T) -> Result<T, Error> {
    if reader.remaining() == 0 {
        Ok(value)
    } else {
        Err(Error::redundant_bytes(0, reader.remaining()))
    }
}

/// `BlocksAndUnclesByHeightsResponse`: request id, then per requested chain
/// the blocks at the requested heights (main chain and uncles).
pub fn decode_blocks_and_uncles_by_heights_response(
    payload: &[u8],
) -> Result<(u32, Vec<Vec<Block>>), Error> {
    let mut reader = Reader::new(payload);
    let id = decode_id(&mut reader)?;
    let blocks = Vec::<Vec<Block>>::decode(&mut reader)?;
    finish(&reader, (id, blocks))
}

pub fn decode_headers_by_heights_response(
    payload: &[u8],
) -> Result<(u32, Vec<Vec<BlockHeader>>), Error> {
    let mut reader = Reader::new(payload);
    let id = decode_id(&mut reader)?;
    let headers = Vec::<Vec<BlockHeader>>::decode(&mut reader)?;
    finish(&reader, (id, headers))
}

/// `NewBlock`: the bare serialized block.
pub fn decode_new_block(payload: &[u8]) -> Result<Block, Error> {
    alephium_model::deserialize(payload)
}

#[cfg(test)]
mod tests {
    use super::{
        HeightRange, decode_blocks_and_uncles_by_heights_response,
        encode_blocks_and_uncles_by_heights_request,
    };

    #[test]
    fn request_encoding_follows_the_official_product_serde() {
        let range = HeightRange {
            from_group: 0,
            to_group: 3,
            from_height: 1,
            to_height: 64,
            step: 1,
        };
        assert!(range.is_valid());
        // id 7, one entry: 0, 3, 1, 64 (two-byte compact: 0x40 0x40), 1.
        assert_eq!(
            encode_blocks_and_uncles_by_heights_request(7, &[range]),
            vec![0x07, 0x01, 0x00, 0x03, 0x01, 0x40, 0x40, 0x01]
        );
        assert!(!HeightRange { step: 0, ..range }.is_valid());
    }

    #[test]
    fn empty_response_decodes_and_trailing_bytes_are_rejected() {
        assert_eq!(
            decode_blocks_and_uncles_by_heights_response(&[0x07, 0x01, 0x00]),
            Ok((7, vec![vec![]]))
        );
        assert!(decode_blocks_and_uncles_by_heights_response(&[0x07, 0x00, 0x00]).is_err());
    }
}
