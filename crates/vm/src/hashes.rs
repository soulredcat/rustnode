//! The hash instructions.

use sha2::Digest;

#[must_use]
pub fn digest(name: &str, bytes: &[u8]) -> Vec<u8> {
    match name {
        "Keccak256" => sha3::Keccak256::digest(bytes).to_vec(),
        "Sha256" => sha2::Sha256::digest(bytes).to_vec(),
        "Sha3" => sha3::Sha3_256::digest(bytes).to_vec(),
        _ => alephium_model::Hash::blake2b(bytes).0.to_vec(),
    }
}
