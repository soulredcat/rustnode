//! `Method`, `StatelessScript`, `StatefulScript` (`protocol/vm/Contract.scala`).

use alephium_serde::Error;

use crate::codec::{Codec, Reader};
use crate::instr::{Context, Instr, decode_instrs, encode_instrs};

const PAY_TO_CONTRACT_ONLY_MASK: u8 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Method {
    pub is_public: bool,
    pub use_preapproved_assets: bool,
    pub use_contract_assets: bool,
    pub use_pay_to_contract_only: bool,
    pub args_length: i32,
    pub locals_length: i32,
    pub return_length: i32,
    pub instrs: Vec<Instr>,
}

impl Method {
    /// `Method.validate`.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.args_length >= 0 && self.locals_length >= self.args_length && self.return_length >= 0
    }

    pub fn encode(&self, out: &mut Vec<u8>) {
        self.is_public.encode(out);
        let first_two_bits = match (self.use_preapproved_assets, self.use_contract_assets) {
            (false, false) => 0,
            (true, true) => 1,
            (false, true) => 2,
            (true, false) => 3,
        };
        let pay_to_contract_only = if self.use_pay_to_contract_only {
            PAY_TO_CONTRACT_ONLY_MASK
        } else {
            0
        };
        out.push(first_two_bits | pay_to_contract_only);
        self.args_length.encode(out);
        self.locals_length.encode(out);
        self.return_length.encode(out);
        encode_instrs(&self.instrs, out);
    }

    pub fn decode(reader: &mut Reader<'_>, context: Context) -> Result<Self, Error> {
        let is_public = bool::decode(reader)?;
        let modifier = reader.byte()?;
        let use_pay_to_contract_only = modifier & PAY_TO_CONTRACT_ONLY_MASK != 0;
        let (use_preapproved_assets, use_contract_assets) =
            match modifier & !PAY_TO_CONTRACT_ONLY_MASK {
                0 => (false, false),
                1 => (true, true),
                2 => (false, true),
                3 => (true, false),
                _ => return Err(Error::wrong_format("Invalid assets modifier")),
            };
        Ok(Self {
            is_public,
            use_preapproved_assets,
            use_contract_assets,
            use_pay_to_contract_only,
            args_length: reader.int()?,
            locals_length: reader.int()?,
            return_length: reader.int()?,
            instrs: decode_instrs(reader, context)?,
        })
    }
}

fn encode_methods(methods: &[Method], out: &mut Vec<u8>) {
    crate::codec::encode_length(methods.len(), out);
    for method in methods {
        method.encode(out);
    }
}

fn decode_methods(reader: &mut Reader<'_>, context: Context) -> Result<Vec<Method>, Error> {
    let length = reader.length()?;
    let mut methods = Vec::with_capacity(length);
    for _ in 0..length {
        methods.push(Method::decode(reader, context)?);
    }
    Ok(methods)
}

/// Asset script (P2SH).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatelessScript {
    pub methods: Vec<Method>,
}

impl Codec for StatelessScript {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_methods(&self.methods, out);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        let methods = decode_methods(reader, Context::Stateless)?;
        let valid = !methods.is_empty()
            && methods
                .iter()
                .all(|method| !method.use_preapproved_assets && method.is_valid());
        if valid {
            Ok(Self { methods })
        } else {
            Err(Error::validation("Invalid script"))
        }
    }
}

/// Transaction script (`TxScript`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatefulScript {
    pub methods: Vec<Method>,
}

impl Codec for StatefulScript {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_methods(&self.methods, out);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        let methods = decode_methods(reader, Context::Stateful)?;
        let valid = methods.first().is_some_and(|method| method.is_public)
            && methods.iter().all(Method::is_valid);
        if valid {
            Ok(Self { methods })
        } else {
            Err(Error::validation("Invalid TxScript"))
        }
    }
}
