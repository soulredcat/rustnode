//! Transactions (`protocol/model/{Transaction,UnsignedTransaction,TxInput,TxOutput}.scala`).

use alephium_serde::Error;
use ethnum::U256;

use crate::codec::{Bytes, Codec, Reader, TimeStamp, serialize};
use crate::hash::Hash;
use crate::lockup::{Byte64, LockupScript, UnlockScript, decode_asset_lockup};
use crate::script::StatefulScript;

/// `AssetOutputRef` / `ContractOutputRef`: 4-byte hint, then the output key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutputRef {
    pub hint: i32,
    pub key: Hash,
}

impl Codec for OutputRef {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.hint.to_be_bytes());
        self.key.encode(out);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            hint: i32::from_be_bytes(reader.array()?),
            key: Hash::decode(reader)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxInput {
    pub output_ref: OutputRef,
    pub unlock_script: UnlockScript,
}

impl Codec for TxInput {
    fn encode(&self, out: &mut Vec<u8>) {
        self.output_ref.encode(out);
        self.unlock_script.encode(out);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            output_ref: OutputRef::decode(reader)?,
            unlock_script: UnlockScript::decode(reader)?,
        })
    }
}

pub type Tokens = Vec<(Hash, U256)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetOutput {
    pub amount: U256,
    /// Never `P2C`.
    pub lockup_script: LockupScript,
    pub lock_time: TimeStamp,
    pub tokens: Tokens,
    pub additional_data: Vec<u8>,
}

impl Codec for AssetOutput {
    fn encode(&self, out: &mut Vec<u8>) {
        self.amount.encode(out);
        self.lockup_script.encode(out);
        self.lock_time.encode(out);
        self.tokens.encode(out);
        crate::codec::encode_bytes(&self.additional_data, out);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            amount: U256::decode(reader)?,
            lockup_script: decode_asset_lockup(reader)?,
            lock_time: TimeStamp::decode(reader)?,
            tokens: Tokens::decode(reader)?,
            additional_data: Bytes::decode(reader)?.0,
        })
    }
}

/// The lockup script of a contract output is serialized as the bare contract id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractOutput {
    pub amount: U256,
    pub contract_id: Hash,
    pub tokens: Tokens,
}

impl Codec for ContractOutput {
    fn encode(&self, out: &mut Vec<u8>) {
        self.amount.encode(out);
        self.contract_id.encode(out);
        self.tokens.encode(out);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            amount: U256::decode(reader)?,
            contract_id: Hash::decode(reader)?,
            tokens: Tokens::decode(reader)?,
        })
    }
}

/// `Either[AssetOutput, ContractOutput]`: flag 0 (left) or 1 (right).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxOutput {
    Asset(AssetOutput),
    Contract(ContractOutput),
}

impl Codec for TxOutput {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Asset(output) => {
                out.push(0);
                output.encode(out);
            }
            Self::Contract(output) => {
                out.push(1);
                output.encode(out);
            }
        }
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        match reader.byte()? {
            0 => Ok(Self::Asset(AssetOutput::decode(reader)?)),
            1 => Ok(Self::Contract(ContractOutput::decode(reader)?)),
            _ => Err(Error::wrong_format("expect 0 or 1 for either flag")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsignedTransaction {
    pub version: u8,
    pub network_id: u8,
    pub script: Option<StatefulScript>,
    /// `GasBox`: non-negative.
    pub gas_amount: i32,
    pub gas_price: U256,
    pub inputs: Vec<TxInput>,
    pub fixed_outputs: Vec<AssetOutput>,
}

impl UnsignedTransaction {
    /// `UnsignedTransaction.id`: Blake2b of the serialized unsigned transaction.
    #[must_use]
    pub fn id(&self) -> Hash {
        Hash::blake2b(&serialize(self))
    }
}

impl Codec for UnsignedTransaction {
    fn encode(&self, out: &mut Vec<u8>) {
        self.version.encode(out);
        self.network_id.encode(out);
        self.script.encode(out);
        self.gas_amount.encode(out);
        self.gas_price.encode(out);
        self.inputs.encode(out);
        self.fixed_outputs.encode(out);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        let version = reader.byte()?;
        let network_id = reader.byte()?;
        let script = Option::<StatefulScript>::decode(reader)?;
        let gas_amount = reader.int()?;
        if gas_amount < 0 {
            return Err(Error::validation(format!("Negative gas {gas_amount}")));
        }
        Ok(Self {
            version,
            network_id,
            script,
            gas_amount,
            gas_price: U256::decode(reader)?,
            inputs: Vec::decode(reader)?,
            fixed_outputs: Vec::decode(reader)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub unsigned: UnsignedTransaction,
    pub script_execution_ok: bool,
    pub contract_inputs: Vec<OutputRef>,
    pub generated_outputs: Vec<TxOutput>,
    pub input_signatures: Vec<Byte64>,
    pub script_signatures: Vec<Byte64>,
}

impl Transaction {
    #[must_use]
    pub fn id(&self) -> Hash {
        self.unsigned.id()
    }
}

impl Codec for Transaction {
    fn encode(&self, out: &mut Vec<u8>) {
        self.unsigned.encode(out);
        self.script_execution_ok.encode(out);
        self.contract_inputs.encode(out);
        self.generated_outputs.encode(out);
        self.input_signatures.encode(out);
        self.script_signatures.encode(out);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            unsigned: UnsignedTransaction::decode(reader)?,
            script_execution_ok: bool::decode(reader)?,
            contract_inputs: Vec::decode(reader)?,
            generated_outputs: Vec::decode(reader)?,
            input_signatures: Vec::decode(reader)?,
            script_signatures: Vec::decode(reader)?,
        })
    }
}

/// `TransactionTemplate`: a mempool transaction, before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionTemplate {
    pub unsigned: UnsignedTransaction,
    pub input_signatures: Vec<Byte64>,
    pub script_signatures: Vec<Byte64>,
}

impl Codec for TransactionTemplate {
    fn encode(&self, out: &mut Vec<u8>) {
        self.unsigned.encode(out);
        self.input_signatures.encode(out);
        self.script_signatures.encode(out);
    }

    fn decode(reader: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(Self {
            unsigned: UnsignedTransaction::decode(reader)?,
            input_signatures: Vec::decode(reader)?,
            script_signatures: Vec::decode(reader)?,
        })
    }
}
