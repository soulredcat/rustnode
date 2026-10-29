//! `GasSchedule` and each instruction's gas (`Instr.scala` gas traits).

use alephium_model::{LockupScript, Val};

/// `maximalGasPerTx`.
pub const MAXIMAL_GAS_PER_TX: u64 = 5_000_000;
/// `GasSchedule.callGas`.
pub const CALL: u64 = 200;
const BASE: u64 = 2;
const VERY_LOW: u64 = 3;
const LOW: u64 = 5;
pub const MID: u64 = 8;
const HIGH: u64 = 10;
/// `GasSchedule.txInputBaseGas`, also `GasDestroy`.
pub const TX_INPUT_BASE: u64 = 2000;
/// `GasSchedule.txOutputBaseGas`.
pub const TX_OUTPUT_BASE: u64 = 4500;
/// `GasSchedule.contractStateUpdateBaseGas`.
pub const CONTRACT_STATE_UPDATE_BASE: u64 = 5000;
const CREATE: u64 = 32_000;
const COPY_CREATE: u64 = 24_000;

/// The gas an instruction charges before it runs; `None` for those whose
/// gas depends on their operands and is charged as they run.
#[must_use]
pub fn simple(name: &str) -> Option<u64> {
    Some(match name {
        "Return" | "DEBUG" | "DevInstr" => 0,
        "ConstTrue"
        | "ConstFalse"
        | "I256Const0"
        | "I256Const1"
        | "I256Const2"
        | "I256Const3"
        | "I256Const4"
        | "I256Const5"
        | "I256ConstN1"
        | "U256Const0"
        | "U256Const1"
        | "U256Const2"
        | "U256Const3"
        | "U256Const4"
        | "U256Const5"
        | "I256Const"
        | "U256Const"
        | "BytesConst"
        | "AddressConst"
        | "Pop"
        | "ByteVecSize"
        | "Dup"
        | "Swap"
        | "BlockHash"
        | "TxGasPrice"
        | "TxGasAmount"
        | "TxGasFee"
        | "NullContractAddress"
        | "ALPHTokenId"
        | "MinimalContractDeposit" => BASE,
        "LoadLocal"
        | "StoreLocal"
        | "BoolNot"
        | "BoolAnd"
        | "BoolOr"
        | "BoolEq"
        | "BoolNeq"
        | "I256Add"
        | "I256Sub"
        | "I256Eq"
        | "I256Neq"
        | "I256Lt"
        | "I256Le"
        | "I256Gt"
        | "I256Ge"
        | "U256Add"
        | "U256Sub"
        | "U256Eq"
        | "U256Neq"
        | "U256Lt"
        | "U256Le"
        | "U256Gt"
        | "U256Ge"
        | "I256ToU256"
        | "U256ToI256"
        | "AddressEq"
        | "AddressNeq"
        | "IsAssetAddress"
        | "IsContractAddress"
        | "Assert"
        | "NetworkId"
        | "BlockTimeStamp"
        | "BlockTarget"
        | "TxId"
        | "TxInputAddressAt"
        | "TxInputsSize"
        | "AssertWithErrorCode"
        | "GetSegregatedSignature"
        | "LoadMutField"
        | "StoreMutField"
        | "SelfContractId"
        | "SelfAddress"
        | "LoadImmField" => VERY_LOW,
        "I256Mul"
        | "I256Div"
        | "I256Mod"
        | "U256Mul"
        | "U256Div"
        | "U256Mod"
        | "NumericBitAnd"
        | "NumericBitOr"
        | "NumericXor"
        | "NumericSHL"
        | "NumericSHR"
        | "VerifyAbsoluteLocktime"
        | "ContractIdToAddress"
        | "LoadLocalByIndex"
        | "StoreLocalByIndex"
        | "GroupOfAddress"
        | "I256RoundInfinityDiv"
        | "U256RoundInfinityDiv"
        | "CallerContractId"
        | "CallerAddress"
        | "IsCalledFromTxScript"
        | "CallerInitialStateHash"
        | "CallerCodeHash"
        | "ContractInitialStateHash"
        | "ContractCodeHash"
        | "LoadMutFieldByIndex"
        | "StoreMutFieldByIndex"
        | "LoadImmFieldByIndex"
        | "ExternalCallerContractId"
        | "ExternalCallerAddress" => LOW,
        "U256ModAdd"
        | "U256ModSub"
        | "U256ModMul"
        | "Jump"
        | "IfTrue"
        | "IfFalse"
        | "VerifyRelativeLocktime"
        | "GetSegregatedWebAuthnSignature" => MID,
        "MethodSelector" => HIGH,
        "MulModN" => 13,
        "AddModN" => 8,
        "VerifyTxSignature"
        | "VerifySecP256K1"
        | "VerifyED25519"
        | "VerifyBIP340Schnorr"
        | "VerifySignature" => 2000,
        "EthEcRecover" => 2500,
        "ApproveAlph"
        | "ApproveToken"
        | "AlphRemaining"
        | "TokenRemaining"
        | "IsPaying"
        | "TransferAlph"
        | "TransferAlphFromSelf"
        | "TransferAlphToSelf"
        | "TransferToken"
        | "TransferTokenFromSelf"
        | "TransferTokenToSelf"
        | "BurnToken"
        | "LockApprovedAssets"
        | "PayGasFee" => 30,
        "CreateContract"
        | "CreateContractWithToken"
        | "CreateContractAndTransferToken"
        | "CreateSubContract"
        | "CreateSubContractWithToken"
        | "CreateSubContractAndTransferToken"
        | "CreateMapEntry"
        | "MigrateSimple"
        | "MigrateWithFields" => CREATE,
        "CopyCreateContract"
        | "CopyCreateContractWithToken"
        | "CopyCreateContractAndTransferToken"
        | "CopyCreateSubContract"
        | "CopyCreateSubContractWithToken"
        | "CopyCreateSubContractAndTransferToken" => COPY_CREATE,
        "ContractExists" => 800,
        "DestroySelf" => TX_INPUT_BASE,
        _ => return None,
    })
}

/// `GasFormula.wordLength`.
#[must_use]
pub const fn words(bytes: usize) -> u64 {
    (bytes as u64).div_ceil(8)
}

/// `GasToByte`.
#[must_use]
pub const fn to_byte(bytes: usize) -> u64 {
    words(bytes)
}

/// `GasHash`.
#[must_use]
pub const fn hash(bytes: usize) -> u64 {
    30 + 6 * words(bytes)
}

/// `GasBytesEq`, also `GasZeros`.
#[must_use]
pub const fn bytes_eq(bytes: usize) -> u64 {
    VERY_LOW + words(bytes)
}

/// `GasBytesConcat`, `GasBytesSlice`, also `GasEncode`.
#[must_use]
pub const fn bytes_concat(bytes: usize) -> u64 {
    VERY_LOW + bytes as u64
}

/// `GasExp`: by the exponent's byte length.
#[must_use]
pub const fn exp(byte_length: usize) -> u64 {
    10 + 50 * byte_length as u64
}

/// `GasLog`.
#[must_use]
pub const fn log(fields: usize) -> u64 {
    100 + 20 * fields as u64
}

/// `GasUniqueAddress`.
#[must_use]
pub const fn unique_address(inputs: usize) -> u64 {
    VERY_LOW + inputs as u64
}

/// `GasSchedule.contractLoadGas`.
#[must_use]
pub const fn contract_load(estimated_size: usize) -> u64 {
    800 + words(estimated_size)
}

/// `GasCall.scriptBaseGas`: code bytes plus a call.
#[must_use]
pub const fn code_size(bytes: usize) -> u64 {
    bytes as u64 + CALL
}

/// `Val.estimateByteSize`.
#[must_use]
pub fn estimate(value: &Val) -> usize {
    match value {
        Val::Bool(_) => 1,
        Val::I256(_) | Val::U256(_) => 32,
        Val::ByteVec(bytes) => bytes.len(),
        Val::Address(LockupScript::P2mpkh { pk_hashes, .. }) => pk_hashes.len() * 32,
        Val::Address(_) => 32,
    }
}

/// `GasCall.fieldsBaseGas` over `VM.checkFieldSize`'s estimate.
#[must_use]
pub fn fields<'v>(values: impl IntoIterator<Item = &'v Val>) -> u64 {
    values.into_iter().map(estimate).sum::<usize>() as u64
}
