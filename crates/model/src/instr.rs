//! VM instructions as bytes: opcode plus operand (`protocol/vm/Instr.scala`).
//!
//! Only the encoding lives here; execution belongs to the VM. Scripts have no
//! length prefix, so decoding a transaction requires decoding every
//! instruction of its scripts.

use alephium_serde::Error;
use ethnum::{I256, U256};

use crate::codec::{Bytes, Codec, Reader, encode_bytes};
use crate::instr_table::INSTRS;
use crate::lockup::LockupScript;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperandKind {
    None,
    Byte,
    /// `ControlCompanion.offsetSerde`: compact int within ±2^16.
    Offset,
    I256,
    U256,
    Bytes,
    Address,
    /// `Method.Selector`: 4-byte big-endian int.
    Selector,
    BytePair,
    DebugParts,
    Dev,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    None,
    Byte(u8),
    Offset(i32),
    I256(I256),
    U256(U256),
    Bytes(Vec<u8>),
    Address(LockupScript),
    Selector(i32),
    BytePair(u8, u8),
    DebugParts(Vec<Vec<u8>>),
    Dev(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instr {
    pub code: u8,
    pub operand: Operand,
}

/// Which instruction set a script is decoded with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// `StatelessContext`: asset scripts (P2SH).
    Stateless,
    /// `StatefulContext`: transaction scripts and contracts.
    Stateful,
}

const OFFSET_LIMIT: i32 = 1 << 16;
const DEV_INSTR_COUNT: u8 = 8;

/// The instruction with this code, if it exists in the context.
#[must_use]
pub fn lookup(code: u8, context: Context) -> Option<(&'static str, OperandKind)> {
    let (_, name, stateful_only, kind) = INSTRS
        .binary_search_by_key(&code, |entry| entry.0)
        .ok()
        .map(|index| INSTRS[index])?;
    if stateful_only && context == Context::Stateless {
        None
    } else {
        Some((name, kind))
    }
}

/// The code of the instruction named `name`.
#[must_use]
pub fn code_of(name: &str) -> Option<u8> {
    INSTRS
        .iter()
        .find(|entry| entry.1 == name)
        .map(|entry| entry.0)
}

impl Instr {
    #[must_use]
    pub fn name(&self) -> &'static str {
        lookup(self.code, Context::Stateful).map_or("Unknown", |(name, _)| name)
    }

    pub fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.code);
        match &self.operand {
            Operand::None => {}
            Operand::Byte(byte) | Operand::Dev(byte) => out.push(*byte),
            Operand::Offset(offset) => offset.encode(out),
            Operand::I256(value) => value.encode(out),
            Operand::U256(value) => value.encode(out),
            Operand::Bytes(bytes) => encode_bytes(bytes, out),
            Operand::Address(lockup) => lockup.encode(out),
            Operand::Selector(selector) => out.extend_from_slice(&selector.to_be_bytes()),
            Operand::BytePair(first, second) => out.extend_from_slice(&[*first, *second]),
            Operand::DebugParts(parts) => {
                crate::codec::encode_length(parts.len(), out);
                for part in parts {
                    encode_bytes(part, out);
                }
            }
        }
    }

    pub fn decode(reader: &mut Reader<'_>, context: Context) -> Result<Self, Error> {
        let code = reader.byte()?;
        let (_, kind) = lookup(code, context)
            .ok_or_else(|| Error::validation(format!("Instruction - invalid code {code}")))?;
        let operand = match kind {
            OperandKind::None => Operand::None,
            OperandKind::Byte => Operand::Byte(reader.byte()?),
            OperandKind::Offset => {
                let offset = reader.int()?;
                if !(-OFFSET_LIMIT..=OFFSET_LIMIT).contains(&offset) {
                    return Err(Error::validation(format!("Invalid offset {offset}")));
                }
                Operand::Offset(offset)
            }
            OperandKind::I256 => Operand::I256(I256::decode(reader)?),
            OperandKind::U256 => Operand::U256(U256::decode(reader)?),
            OperandKind::Bytes => Operand::Bytes(Bytes::decode(reader)?.0),
            OperandKind::Address => Operand::Address(LockupScript::decode(reader)?),
            OperandKind::Selector => Operand::Selector(i32::from_be_bytes(reader.array()?)),
            OperandKind::BytePair => Operand::BytePair(reader.byte()?, reader.byte()?),
            OperandKind::DebugParts => {
                let parts = Vec::<Bytes>::decode(reader)?;
                Operand::DebugParts(parts.into_iter().map(|part| part.0).collect())
            }
            OperandKind::Dev => {
                let dev = reader.byte()?;
                if dev >= DEV_INSTR_COUNT {
                    return Err(Error::wrong_format(format!(
                        "Invalid dev instr prefix {dev}"
                    )));
                }
                Operand::Dev(dev)
            }
        };
        Ok(Self { code, operand })
    }
}

pub(crate) fn encode_instrs(instrs: &[Instr], out: &mut Vec<u8>) {
    crate::codec::encode_length(instrs.len(), out);
    for instr in instrs {
        instr.encode(out);
    }
}

pub(crate) fn decode_instrs(
    reader: &mut Reader<'_>,
    context: Context,
) -> Result<Vec<Instr>, Error> {
    let length = reader.length()?;
    let mut instrs = Vec::with_capacity(length);
    for _ in 0..length {
        instrs.push(Instr::decode(reader, context)?);
    }
    Ok(instrs)
}

#[cfg(test)]
mod tests {
    use super::{Context, OperandKind, lookup};
    use crate::instr_table::INSTRS;

    #[test]
    fn opcode_table_is_sorted_and_contexts_are_respected() {
        assert!(INSTRS.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert_eq!(
            lookup(0, Context::Stateless),
            Some(("CallLocal", OperandKind::Byte))
        );
        assert_eq!(lookup(1, Context::Stateless), None);
        assert_eq!(
            lookup(1, Context::Stateful),
            Some(("CallExternal", OperandKind::Byte))
        );
        assert_eq!(
            lookup(214, Context::Stateful).map(|e| e.0),
            Some("ExternalCallerAddress")
        );
        assert_eq!(lookup(215, Context::Stateful), None);
        assert_eq!(lookup(0xff, Context::Stateful), None);
    }
}
