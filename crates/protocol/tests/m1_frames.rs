use alephium_protocol::message::{
    CURRENT_WIRE_VERSION, FrameDecode, NetworkMagic, decode_frame_exact, decode_frame_prefix,
    decode_raw_message_data, djb_checksum, encode_frame, encode_raw_message_data,
};
use alephium_serde::Error;

const HELLO_FRAME: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/hello-frame.hex"
));
const PING_FRAME: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/ping-frame.hex"
));
const PONG_FRAME: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/pong-frame.hex"
));
const NETWORK_MAGIC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/network-magic.hex"
));
const INVALID_PAYLOAD_CODE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/invalid/invalid-payload-code.hex"
));
const PONG_TRAILING_BYTE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/invalid/pong-trailing-byte.hex"
));
const WRONG_MAGIC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/invalid/wrong-magic.hex"
));
const WRONG_CHECKSUM: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/invalid/wrong-checksum.hex"
));
const NEGATIVE_LENGTH: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/invalid/negative-length.hex"
));
const EXCESSIVE_LENGTH: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/invalid/excessive-length.hex"
));

const MAX_TEST_DATA_LENGTH: usize = 1024 * 1024;

fn decode_hex(value: &str) -> Vec<u8> {
    let value = value.trim();
    assert_eq!(value.len() % 2, 0, "hex fixture must have even length");
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = core::str::from_utf8(pair).expect("fixture hex must be ASCII");
            u8::from_str_radix(text, 16).expect("fixture must contain valid hex")
        })
        .collect()
}

fn magic() -> NetworkMagic {
    let fixture = decode_hex(NETWORK_MAGIC);
    NetworkMagic::new(fixture.try_into().expect("magic fixture must be four bytes"))
}

#[test]
fn canonical_frames_match_official_scala_bytes() {
    for (fixture, expected_code) in [(HELLO_FRAME, 0), (PING_FRAME, 1), (PONG_FRAME, 2)] {
        let encoded = decode_hex(fixture);
        let frame = decode_frame_exact(&encoded, magic(), MAX_TEST_DATA_LENGTH)
            .expect("official frame must decode");

        assert_eq!(
            encode_frame(magic(), frame.data, MAX_TEST_DATA_LENGTH),
            Ok(encoded)
        );

        let raw = decode_raw_message_data(frame.data).expect("raw message data must decode");
        assert_eq!(raw.wire_version, CURRENT_WIRE_VERSION);
        assert_eq!(raw.payload_code, expected_code);
        assert_eq!(
            encode_raw_message_data(raw.payload_code, raw.payload_body),
            Ok(frame.data.to_vec())
        );
    }
}

#[test]
fn canonical_hello_checksum_matches_official_scala_fixture() {
    let encoded = decode_hex(HELLO_FRAME);
    let frame = decode_frame_exact(&encoded, magic(), MAX_TEST_DATA_LENGTH)
        .expect("official Hello frame must decode");
    assert_eq!(frame.checksum as u32, 0xcd44_5fce);
    assert_eq!(djb_checksum(frame.data), frame.checksum);
}

#[test]
fn every_incomplete_hello_prefix_requests_more_data_without_panicking() {
    let encoded = decode_hex(HELLO_FRAME);
    for prefix_len in 0..encoded.len() {
        let result = decode_frame_prefix(&encoded[..prefix_len], magic(), MAX_TEST_DATA_LENGTH)
            .expect("valid prefix must not be a protocol error");
        match result {
            FrameDecode::NeedMoreData { required_total } => {
                assert!(required_total > prefix_len, "prefix {prefix_len}");
            }
            FrameDecode::Complete(_) => panic!("incomplete prefix {prefix_len} decoded as complete"),
        }
    }
}

#[test]
fn prefix_decoder_consumes_one_frame_from_a_multi_frame_buffer() {
    let hello = decode_hex(HELLO_FRAME);
    let ping = decode_hex(PING_FRAME);
    let mut combined = hello.clone();
    combined.extend_from_slice(&ping);

    let first = decode_frame_prefix(&combined, magic(), MAX_TEST_DATA_LENGTH).expect("first frame");
    let FrameDecode::Complete(first) = first else {
        panic!("first frame unexpectedly incomplete");
    };
    assert_eq!(first.consumed, hello.len());

    let second = decode_frame_prefix(&combined[first.consumed..], magic(), MAX_TEST_DATA_LENGTH)
        .expect("second frame");
    let FrameDecode::Complete(second) = second else {
        panic!("second frame unexpectedly incomplete");
    };
    assert_eq!(second.consumed, ping.len());

    assert_eq!(
        decode_frame_exact(&combined, magic(), MAX_TEST_DATA_LENGTH),
        Err(Error::RedundantBytes {
            expected: hello.len(),
            actual: combined.len(),
        })
    );
}

#[test]
fn invalid_envelope_vectors_fail_at_the_official_boundary() {
    assert!(matches!(
        decode_frame_exact(&decode_hex(WRONG_MAGIC), magic(), MAX_TEST_DATA_LENGTH),
        Err(Error::WrongFormat { .. })
    ));
    assert!(matches!(
        decode_frame_exact(
            &decode_hex(WRONG_CHECKSUM),
            magic(),
            MAX_TEST_DATA_LENGTH
        ),
        Err(Error::WrongFormat { .. })
    ));
    assert!(matches!(
        decode_frame_exact(
            &decode_hex(NEGATIVE_LENGTH),
            magic(),
            MAX_TEST_DATA_LENGTH
        ),
        Err(Error::WrongFormat { .. })
    ));
}

#[test]
fn excessive_declared_length_is_bounded_before_data_allocation() {
    let encoded = decode_hex(EXCESSIVE_LENGTH);
    assert_eq!(
        decode_frame_prefix(&encoded, magic(), MAX_TEST_DATA_LENGTH),
        Err(Error::LimitExceeded {
            declared: i32::MAX as usize,
            limit: MAX_TEST_DATA_LENGTH,
        })
    );

    assert_eq!(
        decode_frame_prefix(&encoded, magic(), i32::MAX as usize),
        Ok(FrameDecode::NeedMoreData {
            required_total: 12 + i32::MAX as usize,
        })
    );
}

#[test]
fn payload_code_validation_is_separate_from_outer_frame_validation() {
    let encoded = decode_hex(INVALID_PAYLOAD_CODE);
    let frame = decode_frame_exact(&encoded, magic(), MAX_TEST_DATA_LENGTH)
        .expect("the official outer frame is valid");
    assert!(matches!(
        decode_raw_message_data(frame.data),
        Err(Error::WrongFormat { .. })
    ));
}

#[test]
fn payload_specific_trailing_bytes_are_deferred_to_the_payload_decoder() {
    let encoded = decode_hex(PONG_TRAILING_BYTE);
    let frame = decode_frame_exact(&encoded, magic(), MAX_TEST_DATA_LENGTH)
        .expect("outer frame remains valid");
    let raw = decode_raw_message_data(frame.data).expect("header and payload code remain valid");
    assert_eq!(raw.payload_code, 2);
    assert_eq!(raw.payload_body, &[0x2a, 0x00]);
}
