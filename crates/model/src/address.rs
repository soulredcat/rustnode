//! Addresses, script hints, and output hints and keys
//! (`protocol/model/{Address,Hint,ScriptHint}.scala`, `LockupScript.Groupless`).

use crate::codec::serialize;
use crate::hash::{Hash, djb_hash};
use crate::lockup::LockupScript;

const BASE58_ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Official `Base58.encode`.
#[must_use]
pub fn base58_encode(bytes: &[u8]) -> String {
    let zeros = bytes.iter().take_while(|byte| **byte == 0).count();
    let mut digits: Vec<u8> = Vec::with_capacity(bytes.len() * 138 / 100 + 1);
    for byte in &bytes[zeros..] {
        let mut carry = u32::from(*byte);
        for digit in &mut digits {
            carry += u32::from(*digit) << 8;
            *digit = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let mut text = String::with_capacity(zeros + digits.len());
    text.extend(std::iter::repeat_n('1', zeros));
    text.extend(
        digits
            .iter()
            .rev()
            .map(|digit| char::from(BASE58_ALPHABET[usize::from(*digit)])),
    );
    text
}

/// Official `Base58.decode`; `None` for characters outside the alphabet.
#[must_use]
pub fn base58_decode(input: &str) -> Option<Vec<u8>> {
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

/// `Address.toBase58`: grouped (Danube) lockups print their group after `:`.
#[must_use]
pub fn address_string(lockup: &LockupScript) -> String {
    let bytes = serialize(lockup);
    match lockup {
        LockupScript::P2pk { group_byte, .. } | LockupScript::P2hmpk { group_byte, .. } => {
            format!(
                "{}:{}",
                base58_encode(&bytes[..bytes.len() - 1]),
                *group_byte as i8
            )
        }
        _ => base58_encode(&bytes),
    }
}

/// `ScriptHint.fromHash(Int)`: the low bit is always set.
const fn script_hint_from_int(hash: i32) -> i32 {
    hash | 1
}

/// `Bytes.xorByte`.
const fn xor_byte(value: i32) -> u8 {
    let [b0, b1, b2, b3] = value.to_be_bytes();
    b0 ^ b1 ^ b2 ^ b3
}

/// `LockupScript.scriptHint`.
#[must_use]
pub fn script_hint(lockup: &LockupScript) -> i32 {
    match lockup {
        LockupScript::P2pkh(hash) | LockupScript::P2sh(hash) | LockupScript::P2c(hash) => {
            script_hint_from_int(djb_hash(&hash.0))
        }
        LockupScript::P2mpkh { pk_hashes, .. } => {
            // Decoding guarantees at least one key (0 < m <= n).
            pk_hashes
                .first()
                .map_or(1, |hash| script_hint_from_int(djb_hash(&hash.0)))
        }
        LockupScript::P2pk {
            public_key,
            group_byte,
        } => grouped_script_hint(public_key.raw_bytes(), *group_byte),
        LockupScript::P2hmpk { hash, group_byte } => grouped_script_hint(&hash.0, *group_byte),
    }
}

/// `Groupless.calcScriptHint`: the top byte is rewritten so the hint's group
/// is the explicit group.
fn grouped_script_hint(pre_image: &[u8], group_byte: u8) -> i32 {
    let initial = script_hint_from_int(djb_hash(pre_image));
    let byte0 = initial.to_be_bytes()[0];
    let new_byte0 = byte0 ^ xor_byte(initial) ^ group_byte;
    script_hint_from_int((i32::from(new_byte0) << 24) | (initial & 0x00ff_ffff))
}

/// `LockupScript.groupIndex` (`ScriptHint.groupIndex`).
#[must_use]
pub fn lockup_group(lockup: &LockupScript, groups: u32) -> u32 {
    u32::from(xor_byte(script_hint(lockup))) % groups
}

/// `Hint.ofAsset` / `Hint.ofContract`: contract hints have the low bit cleared.
#[must_use]
pub fn output_hint(lockup: &LockupScript) -> i32 {
    let hint = script_hint(lockup);
    if lockup.is_asset() { hint } else { hint ^ 1 }
}

/// `TxOutputRef.key(txId, index)`.
#[must_use]
pub fn output_key(tx_id: &Hash, index: i32) -> Hash {
    let mut bytes = Vec::with_capacity(36);
    bytes.extend_from_slice(&tx_id.0);
    bytes.extend_from_slice(&index.to_be_bytes());
    Hash::blake2b(&bytes)
}

#[cfg(test)]
mod tests {
    use super::{address_string, base58_decode, base58_encode, output_hint};
    use crate::codec::deserialize;
    use crate::lockup::LockupScript;

    /// Addresses and output hints as printed by the official node.
    #[test]
    fn addresses_and_hints_match_the_official_node() {
        for (address, hint) in [
            (
                "13vtrN3csbfHmz8AXY4fbDU68zYLsgyXdpNSQiCwFKssQ",
                2_012_897_373,
            ),
            (
                "1HZLQntF6eS226zSpHWpUm26c9HNuCNYJ3M1FqaTV6QgC",
                -1_244_970_159,
            ),
            (
                "2jZJbRuqkkJ4wgcbpswjwUVfG9RsnMaf57d3S9hMoYMp2P2GHz78Ce4HVyBoc9zeQycguVyYEdUEwXDzgvBeepZhcy7",
                2_012_897_373,
            ),
        ] {
            let lockup: LockupScript =
                deserialize(&base58_decode(address).expect("base58")).expect("lockup");
            assert_eq!(address_string(&lockup), address);
            assert_eq!(output_hint(&lockup), hint, "{address}");
        }
    }

    #[test]
    fn base58_round_trip() {
        for bytes in [vec![], vec![0], vec![0, 0, 1], vec![255; 33], vec![1, 2, 3]] {
            assert_eq!(base58_decode(&base58_encode(&bytes)), Some(bytes));
        }
        assert_eq!(base58_decode("0OIl"), None);
    }
}
