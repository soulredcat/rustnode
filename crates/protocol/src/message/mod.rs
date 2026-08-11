//! Official Alephium P2P message ownership.

pub mod frame;

pub use frame::{
    CHECKSUM_LENGTH, CURRENT_WIRE_VERSION, ENVELOPE_LENGTH, Frame, FrameDecode,
    LENGTH_FIELD_LENGTH, MAX_PAYLOAD_CODE, MAGIC_LENGTH, NetworkMagic, RawMessageData,
    decode_frame_exact, decode_frame_prefix, decode_raw_message_data, djb_checksum, encode_frame,
    encode_raw_message_data,
};
