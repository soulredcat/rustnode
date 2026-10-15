//! `Val`: VM values (`protocol/vm/Val.scala`).

use alephium_serde::Error;
use ethnum::{I256, U256};

use crate::codec::{Bytes, Codec, Reader};
use crate::lockup::LockupScript;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Val {
    Bool(bool),
    I256(I256),
    U256(U256),
    ByteVec(Vec<u8>),
    Address(LockupScript),
}

impl Val {
    /// `Val.Type.id`.
    #[must_use]
    pub const fn type_id(&self) -> u8 {
        match self {
            Self::Bool(_) => 0,
            Self::I256(_) => 1,
            Self::U256(_) => 2,
            Self::ByteVec(_) => 3,
            Self::Address(_) => 4,
        }
    }
}

impl Codec for Val {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.type_id());
        match self {
            Self::Bool(value) => value.encode(out),
            Self::I256(value) => value.encode(out),
            Self::U256(value) => value.encode(out),
            Self::ByteVec(bytes) => crate::codec::encode_bytes(bytes, out),
            Self::Address(lockup) => lockup.encode(out),
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        match reader.byte()? {
            0 => Ok(Self::Bool(bool::decode(reader)?)),
            1 => Ok(Self::I256(I256::decode(reader)?)),
            2 => Ok(Self::U256(U256::decode(reader)?)),
            3 => Ok(Self::ByteVec(Bytes::decode(reader)?.0)),
            4 => Ok(Self::Address(LockupScript::decode(reader)?)),
            code => Err(Error::wrong_format(format!("Invalid type id: {code}"))),
        }
    }
}
