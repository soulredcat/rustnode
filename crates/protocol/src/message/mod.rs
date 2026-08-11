//! Official Alephium P2P message ownership.

pub mod payload;
pub mod frame;

pub use frame::{
    CHECKSUM_LENGTH, CURRENT_WIRE_VERSION, ENVELOPE_LENGTH, Frame, FrameDecode,
    LENGTH_FIELD_LENGTH, MAGIC_LENGTH, MAX_PAYLOAD_CODE, NetworkMagic, RawMessageData,
    decode_frame_exact, decode_frame_prefix, decode_raw_message_data, djb_checksum, encode_frame,
    encode_raw_message_data,
};
pub use payload::{
    CliqueId, Header, Hello, HELLO_CODE, InterBrokerInfo, PING_CODE, PONG_CODE, Ping, P2PVersion,
    PayloadCode, ReleaseVersion, RequestId, Pong, decode_hello_payload, decode_ping_payload,
    decode_pong_payload, decode_wire_header, encode_hello_payload, encode_ping_payload, encode_pong_payload,
    encode_wire_header,
};
