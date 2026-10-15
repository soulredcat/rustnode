//! Group of an asset address, computed exactly like the official
//! `LockupScript.Asset.groupIndex` (pinned baseline, `protocol/vm/LockupScript.scala`
//! and `protocol/model/ScriptHint.scala`).
//!
//! Only P2PKH, P2MPKH, and P2SH addresses are handled. Contract addresses need
//! contract state, and grouped (`base58:group`) addresses need checksum
//! validation; both return `None` so the official node answers them.

use alephium_protocol::message::djb_checksum;
use alephium_serde::compact;

const BASE58_ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
/// Longer inputs are left to the official node; real addresses are far shorter.
const MAX_ADDRESS_CHARS: usize = 512;
const HASH_BYTES: usize = 32;

const P2PKH_PREFIX: u8 = 0;
const P2MPKH_PREFIX: u8 = 1;
const P2SH_PREFIX: u8 = 2;

/// The official `GET /addresses/{address}/group` result for an asset address,
/// or `None` when this address must be answered by the official node.
#[must_use]
pub(crate) fn asset_address_group(address: &str, groups: u32) -> Option<u32> {
    if groups == 0 || address.contains(':') {
        return None;
    }
    let bytes = base58_decode(address)?;
    let (&prefix, script) = bytes.split_first()?;
    let hint_hash = match prefix {
        P2PKH_PREFIX | P2SH_PREFIX => single_hash(script)?,
        P2MPKH_PREFIX => first_multisig_hash(script)?,
        _ => return None,
    };
    Some(script_hint_group(hint_hash, groups))
}

fn single_hash(script: &[u8]) -> Option<&[u8]> {
    (script.len() == HASH_BYTES).then_some(script)
}

/// `P2MPKH` serde: compact-int count, that many 32-byte hashes, compact-int `m`,
/// with `0 < m <= count` and no trailing bytes. The hint comes from the first hash.
fn first_multisig_hash(script: &[u8]) -> Option<&[u8]> {
    let count = compact::decode_i32(script).ok()?;
    let keys = usize::try_from(count.value).ok().filter(|keys| *keys > 0)?;
    let hashes_len = keys.checked_mul(HASH_BYTES)?;
    if count.rest.len() < hashes_len {
        return None;
    }
    let (hashes, rest) = count.rest.split_at(hashes_len);
    let threshold = compact::decode_i32(rest).ok()?;
    let threshold_valid = usize::try_from(threshold.value)
        .ok()
        .is_some_and(|m| m > 0 && m <= keys);
    (threshold_valid && threshold.rest.is_empty()).then(|| &hashes[..HASH_BYTES])
}

/// `ScriptHint.fromHash(hash).groupIndex`: DJB hash of the 32 bytes with the low
/// bit set, its four bytes XOR-ed together, taken as unsigned, modulo `groups`.
fn script_hint_group(hash: &[u8], groups: u32) -> u32 {
    let hint = djb_checksum(hash) | 1;
    let [b0, b1, b2, b3] = hint.to_be_bytes();
    u32::from(b0 ^ b1 ^ b2 ^ b3) % groups
}

/// Official `Base58.decode`: leading `1`s become zero bytes, the rest is a
/// big-endian base-58 number without leading zero bytes.
fn base58_decode(input: &str) -> Option<Vec<u8>> {
    if input.is_empty() || input.len() > MAX_ADDRESS_CHARS {
        return None;
    }
    let zeros = input.bytes().take_while(|byte| *byte == b'1').count();
    let mut number: Vec<u8> = Vec::with_capacity(input.len());
    for character in input.bytes().skip(zeros) {
        let digit = BASE58_ALPHABET.iter().position(|c| *c == character)?;
        let mut carry = u32::try_from(digit).ok()?;
        for byte in number.iter_mut().rev() {
            carry += u32::from(*byte) * 58;
            *byte = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            number.insert(0, (carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    let mut decoded = vec![0u8; zeros];
    decoded.extend(number.into_iter().skip_while(|byte| *byte == 0));
    Some(decoded)
}

#[cfg(test)]
mod tests {
    use super::{asset_address_group, base58_decode};

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
            .collect()
    }

    /// Test-only encoder matching official `Base58.encode`.
    fn base58_encode(bytes: &[u8]) -> String {
        let zeros = bytes.iter().take_while(|byte| **byte == 0).count();
        let mut digits: Vec<u8> = Vec::new();
        for byte in &bytes[zeros..] {
            let mut carry = u32::from(*byte);
            for digit in digits.iter_mut() {
                carry += u32::from(*digit) << 8;
                *digit = (carry % 58) as u8;
                carry /= 58;
            }
            while carry > 0 {
                digits.push((carry % 58) as u8);
                carry /= 58;
            }
        }
        let mut text = "1".repeat(zeros);
        text.extend(
            digits
                .iter()
                .rev()
                .map(|digit| char::from(super::BASE58_ALPHABET[usize::from(*digit)])),
        );
        text
    }

    /// `AddressSpec."calculate group index"` and
    /// `AddressSpec."encode and decode between p2pkh address and public key"`
    /// at the pinned baseline: address, public key, expected group. Those
    /// official tests run with `GroupConfigFixture.Default`, i.e. 3 groups.
    const SPEC_GROUPS: u32 = 3;
    const P2PKH: [(&str, &str, u32); 4] = [
        (
            "1C2RAVWSuaXw8xtUxqVERR7ChKBE1XgscNFw73NSHE1v3",
            "02a16415ccabeb3bc1ee21daacdd53b780fb287afc1f9ab02ae21bb7559d84dd10",
            2,
        ),
        (
            "1H7CmpbvGJwgyLzR91wzSJJSkiBC92WDPTWny4gmhQJQc",
            "03c83325bd2c0fe1464161c6d5f42699fc9dd799dda7f984f9fbf59b01b095be19",
            0,
        ),
        (
            "1DkrQMni2h8KYpvY8t7dECshL66gwnxiR5uD2Udxps6og",
            "03c0a849d8ab8633b45b45ea7f3bb3229e1083a13fd73e027aac2bc55e7f622172",
            1,
        ),
        (
            "131R8ufDhcsu6SRztR9D3m8GUzkWFUPfT78aQ6jgtgzob",
            "026a1552ddf754abbfed6784f709fc94b7fe96049939986ea31e46238849953d18",
            0,
        ),
    ];

    #[test]
    fn p2pkh_addresses_match_official_vectors() {
        for (address, public_key, group) in P2PKH {
            let decoded = base58_decode(address).expect("decode");
            let mut expected = vec![0u8];
            expected.extend(alephium_crypto::hash_blake2b_256(&hex(public_key)));
            assert_eq!(decoded, expected, "{address}");
            assert_eq!(
                asset_address_group(address, SPEC_GROUPS),
                Some(group),
                "{address}"
            );
            assert_eq!(base58_encode(&decoded), address);
        }
    }

    /// `AddressSpec."encode and decode between p2mpkh address and public keys"`:
    /// 1-of-2 over the first two keys above. The hint comes from the first key,
    /// whose P2PKH group is 2 with 3 groups.
    #[test]
    fn p2mpkh_address_uses_first_key_hint() {
        let address = "2jjvDdgGjC6X9HHMCMHohVfvp1uf3LHQrAGWaufR17P7AFwtxodTxSktqKc2urNEtaoUCy5xXpBUwpZ8QM8Q3e5BYCx";
        let decoded = base58_decode(address).expect("decode");
        let mut expected = vec![1u8, 2];
        expected.extend(alephium_crypto::hash_blake2b_256(&hex(P2PKH[0].1)));
        expected.extend(alephium_crypto::hash_blake2b_256(&hex(P2PKH[1].1)));
        expected.push(1);
        assert_eq!(decoded, expected);
        assert_eq!(asset_address_group(address, SPEC_GROUPS), Some(2));
        assert_eq!(
            asset_address_group(address, 4),
            asset_address_group(P2PKH[0].0, 4)
        );
    }

    #[test]
    fn p2sh_addresses_decode_to_a_group() {
        for address in [
            "je9CrJD444xMSGDA2yr1XMvugoHuTc6pfYEaPYrKLuYa",
            "rvpeCy7GhsGHq8n6TnB1LjQh4xn1FMHJVXnsdZAniKZA",
        ] {
            let decoded = base58_decode(address).expect("decode");
            assert_eq!(decoded.len(), 33);
            assert_eq!(decoded[0], 2);
            assert!(asset_address_group(address, 4).is_some_and(|group| group < 4));
        }
    }

    #[test]
    fn addresses_needing_the_official_node_are_declined() {
        let mut contract = vec![3u8];
        contract.extend([0x11; 32]);
        let contract = base58_encode(&contract);
        let mut malformed_multisig = vec![1u8, 2];
        malformed_multisig.extend([0x22; 64]);
        malformed_multisig.push(3);
        let malformed_multisig = base58_encode(&malformed_multisig);

        for address in [
            contract.as_str(),
            malformed_multisig.as_str(),
            "1C2RAVWSuaXw8xtUxqVERR7ChKBE1XgscNFw73NSHE1v3:2",
            "1C2RAVWSuaXw8xtUxqVERR7ChKBE1Xg",
            "1C2RAVWSuaXw8xtUxqVERR7ChKBE1XgscNFw73NSHE1v3zz",
            "0C2RAVWSuaXw8xtUxqVERR7ChKBE1XgscNFw73NSHE1v3",
            "",
        ] {
            assert_eq!(asset_address_group(address, 4), None, "{address}");
        }
        assert_eq!(asset_address_group(P2PKH[0].0, 0), None);
        assert_eq!(asset_address_group(&"1".repeat(600), 4), None);
    }

    #[test]
    fn base58_keeps_leading_zero_bytes() {
        assert_eq!(base58_decode("1"), Some(vec![0]));
        assert_eq!(base58_decode("11"), Some(vec![0, 0]));
        assert_eq!(base58_decode("1112"), Some(vec![0, 0, 0, 1]));
        assert_eq!(base58_decode("z"), Some(vec![57]));
        assert_eq!(base58_decode("21"), Some(vec![58]));
        assert_eq!(base58_decode("I"), None);
    }
}
