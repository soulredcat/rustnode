//! `LockupScript`, `PublicKeyLike`, and `UnlockScript` (`protocol/vm`).

use alephium_serde::Error;

use crate::codec::{Codec, Reader};
use crate::hash::{Hash, djb_hash};
use crate::script::StatelessScript;
use crate::val::Val;

pub type PublicKey = [u8; 33];
pub type Byte64 = [u8; 64];

/// `PublicKeyLike`: the key types of Danube's grouped (`P2PK`, `P2HMPK`)
/// addresses.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PublicKeyLike {
    SecP256K1([u8; 33]),
    SecP256R1([u8; 33]),
    Ed25519([u8; 32]),
    WebAuthn([u8; 33]),
}

impl PublicKeyLike {
    #[must_use]
    pub fn raw_bytes(&self) -> &[u8] {
        match self {
            Self::SecP256K1(key) | Self::SecP256R1(key) | Self::WebAuthn(key) => key,
            Self::Ed25519(key) => key,
        }
    }
}

impl Codec for PublicKeyLike {
    fn encode(&self, out: &mut Vec<u8>) {
        let tag = match self {
            Self::SecP256K1(_) => 0,
            Self::SecP256R1(_) => 1,
            Self::Ed25519(_) => 2,
            Self::WebAuthn(_) => 3,
        };
        out.push(tag);
        out.extend_from_slice(self.raw_bytes());
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        match reader.byte()? {
            0 => Ok(Self::SecP256K1(reader.array()?)),
            1 => Ok(Self::SecP256R1(reader.array()?)),
            2 => Ok(Self::Ed25519(reader.array()?)),
            3 => Ok(Self::WebAuthn(reader.array()?)),
            tag => Err(Error::wrong_format(format!(
                "Invalid public key type {tag}"
            ))),
        }
    }
}

/// `Groupless.safeSerde`: the value's bytes followed by their 4-byte DJB checksum.
fn encode_with_checksum<T: Codec>(value: &T, out: &mut Vec<u8>) {
    let start = out.len();
    value.encode(out);
    let checksum = djb_hash(&out[start..]).to_be_bytes();
    out.extend_from_slice(&checksum);
}

fn decode_with_checksum<T: Codec>(reader: &mut Reader<'_>) -> Result<T, Error> {
    let before = reader.rest();
    let value = T::decode(reader)?;
    let raw = &before[..before.len() - reader.remaining()];
    let checksum: [u8; 4] = reader.array()?;
    let expected = djb_hash(raw).to_be_bytes();
    if checksum == expected {
        Ok(value)
    } else {
        Err(Error::wrong_format(format!(
            "Wrong checksum: expected {}, got {}",
            crate::hash::hex(&expected),
            crate::hash::hex(&checksum)
        )))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LockupScript {
    P2pkh(Hash),
    /// m-of-n; validated `0 < m <= n` on decode.
    P2mpkh {
        pk_hashes: Vec<Hash>,
        m: i32,
    },
    P2sh(Hash),
    /// Contract id.
    P2c(Hash),
    P2pk {
        public_key: PublicKeyLike,
        group_byte: u8,
    },
    P2hmpk {
        hash: Hash,
        group_byte: u8,
    },
}

impl LockupScript {
    #[must_use]
    pub const fn is_asset(&self) -> bool {
        !matches!(self, Self::P2c(_))
    }
}

impl Codec for LockupScript {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::P2pkh(hash) => {
                out.push(0);
                hash.encode(out);
            }
            Self::P2mpkh { pk_hashes, m } => {
                out.push(1);
                pk_hashes.encode(out);
                m.encode(out);
            }
            Self::P2sh(hash) => {
                out.push(2);
                hash.encode(out);
            }
            Self::P2c(contract_id) => {
                out.push(3);
                contract_id.encode(out);
            }
            Self::P2pk {
                public_key,
                group_byte,
            } => {
                out.push(4);
                encode_with_checksum(public_key, out);
                out.push(*group_byte);
            }
            Self::P2hmpk { hash, group_byte } => {
                out.push(5);
                encode_with_checksum(hash, out);
                out.push(*group_byte);
            }
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        match reader.byte()? {
            0 => Ok(Self::P2pkh(Hash::decode(reader)?)),
            1 => {
                let pk_hashes = Vec::<Hash>::decode(reader)?;
                let m = reader.int()?;
                let n = i32::try_from(pk_hashes.len()).unwrap_or(i32::MAX);
                if m > 0 && m <= n {
                    Ok(Self::P2mpkh { pk_hashes, m })
                } else {
                    Err(Error::validation(format!(
                        "Invalid m in m-of-n multisig: m={m}, n={n}"
                    )))
                }
            }
            2 => Ok(Self::P2sh(Hash::decode(reader)?)),
            3 => Ok(Self::P2c(Hash::decode(reader)?)),
            4 => Ok(Self::P2pk {
                public_key: decode_with_checksum(reader)?,
                group_byte: reader.byte()?,
            }),
            5 => Ok(Self::P2hmpk {
                hash: decode_with_checksum(reader)?,
                group_byte: reader.byte()?,
            }),
            prefix => Err(Error::wrong_format(format!(
                "Invalid lockupScript prefix {prefix}"
            ))),
        }
    }
}

/// `LockupScript.Asset`: any lockup but `P2C`.
pub fn decode_asset_lockup(reader: &mut Reader<'_>) -> Result<LockupScript, Error> {
    let lockup = LockupScript::decode(reader)?;
    if lockup.is_asset() {
        Ok(lockup)
    } else {
        Err(Error::validation(
            "Expect LockupScript.Asset, but was LockupScript.P2C",
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnlockScript {
    P2pkh(PublicKey),
    P2mpkh(Vec<(PublicKey, i32)>),
    P2sh {
        script: StatelessScript,
        params: Vec<Val>,
    },
    SameAsPrevious,
    PoLW(PublicKey),
    P2pk,
    P2hmpk {
        public_keys: Vec<PublicKeyLike>,
        public_key_indexes: Vec<i32>,
    },
}

impl Codec for UnlockScript {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::P2pkh(key) => {
                out.push(0);
                key.encode(out);
            }
            Self::P2mpkh(keys) => {
                out.push(1);
                keys.encode(out);
            }
            Self::P2sh { script, params } => {
                out.push(2);
                script.encode(out);
                params.encode(out);
            }
            Self::SameAsPrevious => out.push(3),
            Self::PoLW(key) => {
                out.push(4);
                key.encode(out);
            }
            Self::P2pk => out.push(5),
            Self::P2hmpk {
                public_keys,
                public_key_indexes,
            } => {
                out.push(6);
                public_keys.encode(out);
                public_key_indexes.encode(out);
            }
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        match reader.byte()? {
            0 => Ok(Self::P2pkh(reader.array()?)),
            1 => Ok(Self::P2mpkh(Vec::decode(reader)?)),
            2 => Ok(Self::P2sh {
                script: StatelessScript::decode(reader)?,
                params: Vec::decode(reader)?,
            }),
            3 => Ok(Self::SameAsPrevious),
            4 => Ok(Self::PoLW(reader.array()?)),
            5 => Ok(Self::P2pk),
            6 => {
                let public_keys = Vec::<PublicKeyLike>::decode(reader)?;
                let public_key_indexes = Vec::<i32>::decode(reader)?;
                validate_p2hmpk(&public_keys, &public_key_indexes).map_err(Error::validation)?;
                Ok(Self::P2hmpk {
                    public_keys,
                    public_key_indexes,
                })
            }
            prefix => Err(Error::wrong_format(format!(
                "Invalid unlock script prefix {prefix}"
            ))),
        }
    }
}

/// `UnlockScript.P2HMPK.validate`.
fn validate_p2hmpk(public_keys: &[PublicKeyLike], indexes: &[i32]) -> Result<(), &'static str> {
    if public_keys.is_empty() {
        return Err("Public keys can not be empty");
    }
    if indexes.is_empty() {
        return Err("Public key indexes can not be empty");
    }
    if indexes.len() > public_keys.len() {
        return Err("Public key indexes length can not be greater than public keys length");
    }
    let keys = i64::try_from(public_keys.len()).unwrap_or(i64::MAX);
    let sorted_in_range = indexes.iter().enumerate().all(|(i, index)| {
        let index = i64::from(*index);
        let in_range = index >= 0 && index < keys;
        match indexes.get(i + 1) {
            Some(next) => in_range && index < i64::from(*next),
            None => in_range,
        }
    });
    if sorted_in_range {
        Ok(())
    } else {
        Err(
            "Public key indexes should be sorted in ascending order, each index should be in range [0, publicKeys.length)",
        )
    }
}
