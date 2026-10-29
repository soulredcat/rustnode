mod common;

use alephium_crypto::{PublicKey, SIGNATURE_LENGTH, Signature, verify};
use alephium_protocol::message::payload::{HELLO_CODE, PING_CODE, PONG_CODE};
use alephium_protocol::message::{
    CURRENT_WIRE_VERSION, NetworkMagic, Ping, Pong, RequestId, decode_hello_payload,
    decode_ping_payload, decode_pong_payload, decode_raw_message_data, encode_hello_payload,
    encode_ping_payload, encode_pong_payload, encode_raw_message_data,
};
use common::decode_frame_exact;

const BROKER_INFO_HASH: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/broker-info-hash.hex"
));
const BROKER_ID: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/clique-id.hex"
));
const HELLO_PAYLOAD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/hello-payload.hex"
));
const HELLO_INVALID_BROKER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/invalid/hello-invalid-broker.hex"
));
const HELLO_INVALID_SIGNATURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/invalid/hello-invalid-signature.hex"
));
const HELLO_ZERO_TIMESTAMP: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/invalid/hello-zero-timestamp.hex"
));
const PING_PAYLOAD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/ping-payload.hex"
));
const PONG_PAYLOAD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/pong-payload.hex"
));
const WIRE_HEADER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/wire-header.hex"
));
const NETWORK_MAGIC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/network-magic.hex"
));

const MAX_DATA_LENGTH: usize = 1024 * 1024;

fn decode_hex(value: &str) -> Vec<u8> {
    let value = value.trim();
    assert_eq!(value.len() % 2, 0, "hex fixture must have even length");
    let mut output = Vec::with_capacity(value.len() / 2);
    for i in (0..value.len()).step_by(2) {
        output.push(u8::from_str_radix(&value[i..i + 2], 16).expect("fixture must be valid hex"));
    }
    output
}

fn magic() -> NetworkMagic {
    let magic = decode_hex(NETWORK_MAGIC);
    NetworkMagic::new(
        magic
            .try_into()
            .expect("network magic fixture must be four bytes"),
    )
}

fn split_payload(bytes: &[u8]) -> (i32, &[u8]) {
    let (code, body) = bytes
        .split_first()
        .expect("payload fixture must contain payload code");
    (*code as i32, body)
}

#[test]
fn raw_message_data_uses_official_wire_header() {
    let header = decode_hex(WIRE_HEADER);
    let encoded = encode_raw_message_data(PING_CODE, &[]).expect("encode raw message data");
    assert!(encoded.starts_with(&header));

    let raw = decode_raw_message_data(&encoded).expect("decode raw message data");
    assert_eq!(raw.wire_version, CURRENT_WIRE_VERSION);
    assert_eq!(raw.payload_code, PING_CODE);
    assert!(raw.payload_body.is_empty());
}

#[test]
fn hello_payload_roundtrips_and_passes_handshake_validation() {
    let payload = decode_hex(HELLO_PAYLOAD);
    let (payload_code, raw) = split_payload(&payload);
    assert_eq!(payload_code, HELLO_CODE);

    let hello = decode_hello_payload(raw, 1).expect("valid hello should decode and validate");
    assert_eq!(hello.client_id, "rust-alephium/v4.0.0/linux/p2p-v1");
    assert_eq!(hello.timestamp, 1_700_000_000_000);
    assert_eq!(hello.broker_info.broker_id, 0);
    assert_eq!(hello.broker_info.broker_num, 1);
    let expected_clique_id: [u8; alephium_crypto::PUBLIC_KEY_LENGTH] =
        decode_hex(BROKER_ID).try_into().expect("clique-id size");
    assert_eq!(hello.broker_info.clique_id.bytes, expected_clique_id);
    assert_eq!(hello.signature.len(), SIGNATURE_LENGTH);

    let expected_hash = decode_hex(BROKER_INFO_HASH);
    assert_eq!(
        hello.broker_info.hash().as_slice(),
        expected_hash.as_slice()
    );

    let signature =
        Signature::try_from(hello.signature.as_slice()).expect("signature must be 64 bytes");
    let pubkey = PublicKey::try_from(hello.broker_info.clique_id.bytes.as_slice())
        .expect("clique public key fixture should be valid");
    assert!(verify(&hello.broker_info.hash(), &signature, &pubkey));
    assert_eq!(
        encode_hello_payload(&hello).expect("valid hello should re-encode"),
        raw
    );
}

#[test]
fn hello_invalid_variants_are_rejected_with_validation_errors() {
    for (path, expectation) in [
        (HELLO_ZERO_TIMESTAMP, "timestamp"),
        (HELLO_INVALID_SIGNATURE, "signature"),
        (HELLO_INVALID_BROKER, "broker"),
    ] {
        let bytes = decode_hex(path);
        let frame = decode_frame_exact(&bytes, magic(), MAX_DATA_LENGTH)
            .expect("invalid hello fixture must be a valid frame");
        let raw = decode_raw_message_data(frame.data)
            .expect("raw message header in fixture must be valid");
        assert_eq!(raw.wire_version, CURRENT_WIRE_VERSION);
        assert_eq!(raw.payload_code, HELLO_CODE);
        let error = decode_hello_payload(raw.payload_body, 1).expect_err("invalid hello must fail");
        assert!(
            matches!(error, alephium_serde::Error::Validation { .. }),
            "expected Validation for {expectation}, got {error:?}"
        );
    }
}

#[test]
fn ping_and_pong_payload_codecs_match_fixtures_and_roundtrip() {
    let ping_raw = decode_hex(PING_PAYLOAD);
    let (ping_code, ping_body) = split_payload(&ping_raw);
    assert_eq!(ping_code, PING_CODE);
    let ping = decode_ping_payload(ping_body)
        .expect("ping fixture should decode")
        .value;
    assert_eq!(
        ping,
        Ping {
            id: RequestId { value: 42 },
            timestamp: 1_700_000_001_000,
        }
    );
    assert_eq!(encode_ping_payload(&ping), ping_body);

    let pong_raw = decode_hex(PONG_PAYLOAD);
    let (pong_code, pong_body) = split_payload(&pong_raw);
    assert_eq!(pong_code, PONG_CODE);
    let pong = decode_pong_payload(pong_body)
        .expect("pong fixture should decode")
        .value;
    assert_eq!(
        pong,
        Pong {
            id: RequestId { value: 42 }
        }
    );
    assert_eq!(encode_pong_payload(&pong), pong_body);
}
