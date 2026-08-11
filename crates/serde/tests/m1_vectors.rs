use alephium_serde::{Error, compact};

const SIGNED_VECTORS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/compact-signed-int.tsv"
));
const UNSIGNED_VECTORS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/m1/generated/compact-unsigned-u32.tsv"
));

fn decode_hex(value: &str) -> Vec<u8> {
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

#[test]
fn signed_compact_encoding_matches_official_scala_vectors() {
    for line in SIGNED_VECTORS.lines().skip(1) {
        let (decimal, expected_hex) = line
            .split_once('\t')
            .expect("signed fixture row must contain two columns");
        let value = decimal.parse::<i32>().expect("signed fixture must fit i32");
        let expected = decode_hex(expected_hex);

        assert_eq!(compact::encode_i32(value), expected, "value {value}");

        let mut with_rest = expected.clone();
        with_rest.extend_from_slice(&[0xaa, 0xbb]);
        let decoded = compact::decode_i32(&with_rest).expect("official fixture must decode");
        assert_eq!(decoded.value, value, "value {value}");
        assert_eq!(decoded.rest, &[0xaa, 0xbb], "value {value}");

        for prefix_len in 0..expected.len() {
            assert!(
                matches!(
                    compact::decode_i32(&expected[..prefix_len]),
                    Err(Error::NotEnoughBytes { .. })
                ),
                "truncated signed value {value} at prefix {prefix_len}"
            );
        }
    }
}

#[test]
fn unsigned_compact_encoding_matches_official_scala_vectors() {
    for line in UNSIGNED_VECTORS.lines().skip(1) {
        let mut columns = line.split('\t');
        let decimal = columns.next().expect("unsigned decimal column");
        let _raw_i32 = columns.next().expect("raw i32 column");
        let expected_hex = columns.next().expect("hex column");
        assert!(
            columns.next().is_none(),
            "unexpected unsigned fixture column"
        );

        let value = decimal
            .parse::<u32>()
            .expect("unsigned fixture must fit u32");
        let expected = decode_hex(expected_hex);

        assert_eq!(compact::encode_u32(value), expected, "value {value}");

        let mut with_rest = expected.clone();
        with_rest.extend_from_slice(&[0xaa, 0xbb]);
        let decoded = compact::decode_u32(&with_rest).expect("official fixture must decode");
        assert_eq!(decoded.value, value, "value {value}");
        assert_eq!(decoded.rest, &[0xaa, 0xbb], "value {value}");

        for prefix_len in 0..expected.len() {
            assert!(
                matches!(
                    compact::decode_u32(&expected[..prefix_len]),
                    Err(Error::NotEnoughBytes { .. })
                ),
                "truncated unsigned value {value} at prefix {prefix_len}"
            );
        }
    }
}
