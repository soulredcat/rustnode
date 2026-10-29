//! Shared by the M1 vector tests: a whole input decoded as exactly one frame,
//! on top of the runtime's streaming `decode_frame_prefix`.

use alephium_protocol::message::{Frame, FrameDecode, NetworkMagic, decode_frame_prefix};
use alephium_serde::Error;

pub fn decode_frame_exact(
    input: &[u8],
    magic: NetworkMagic,
    max_data_len: usize,
) -> Result<Frame<'_>, Error> {
    match decode_frame_prefix(input, magic, max_data_len)? {
        FrameDecode::NeedMoreData { required_total } => {
            Err(Error::not_enough_bytes(required_total, input.len()))
        }
        FrameDecode::Complete(frame) if frame.consumed == input.len() => Ok(frame),
        FrameDecode::Complete(frame) => Err(Error::redundant_bytes(frame.consumed, input.len())),
    }
}
