//! The official API's JSON for blocks and transactions
//! (`api/model/{BlockEntry,Transaction,UnsignedTx,AssetInput,Output}.scala`).

use alephium_model::address::{address_string, output_hint, output_key};
use alephium_model::codec::Reader;
use alephium_model::hash::hex;
use alephium_model::lockup::decode_asset_lockup;
use alephium_model::network::{HardFork, Network};
use alephium_model::tx::Tokens;
use alephium_model::{
    AssetOutput, Block, BlockHeader, Codec, Hash, LockupScript, OutputRef, TimeStamp, Transaction,
    TxInput, TxOutput, UnsignedTransaction, serialize,
};
use serde_json::{Value, json};

/// `ALPH.GenesisTimestamp`.
pub const GENESIS_TIMESTAMP: TimeStamp = TimeStamp(1_231_006_505_000);

/// `BlockEntry.from`. `None` when the coinbase data cannot be decoded, where
/// the official node answers with an error.
pub fn block_entry(
    block: &Block,
    height: u32,
    groups: u32,
    network: &Network,
    conflicted_txs: Option<&[Hash]>,
) -> Option<Value> {
    let header = &block.header;
    let hash = block.hash();
    let (chain_from, chain_to) = BlockHeader::chain_index(&hash, groups);
    let ghost_uncles = ghost_uncles(block, network)?;
    let mut entry = json!({
        "hash": hash.to_hex(),
        "timestamp": header.timestamp.0,
        "chainFrom": chain_from,
        "chainTo": chain_to,
        "height": height,
        "deps": header.deps.iter().map(Hash::to_hex).collect::<Vec<_>>(),
        "transactions": block.transactions.iter().map(transaction).collect::<Vec<_>>(),
        "nonce": hex(&header.nonce),
        "version": header.version,
        "depStateHash": header.dep_state_hash.to_hex(),
        "txsHash": header.txs_hash.to_hex(),
        "target": hex(&header.target.0),
        "ghostUncles": ghost_uncles,
    });
    if let Some(conflicted) = conflicted_txs {
        entry["conflictedTxs"] = conflicted.iter().map(Hash::to_hex).collect();
    }
    Some(entry)
}

/// `Block.ghostUncleData`, as `GhostUncleBlockEntry` JSON.
fn ghost_uncles(block: &Block, network: &Network) -> Option<Vec<Value>> {
    if block.header.timestamp == GENESIS_TIMESTAMP {
        return Some(Vec::new());
    }
    let prefix_length = match network.hard_fork(block.header.timestamp) {
        HardFork::Danube => 2,
        HardFork::Rhone => 2 + 8,
        HardFork::Leman | HardFork::Mainnet => return Some(Vec::new()),
    };
    let coinbase = block.transactions.last()?;
    let data = &coinbase.unsigned.fixed_outputs.first()?.additional_data;
    let mut reader = Reader::new(data);
    reader.take(prefix_length).ok()?;
    let count = reader.length().ok()?;
    let mut uncles = Vec::with_capacity(count);
    for _ in 0..count {
        let block_hash = Hash::decode(&mut reader).ok()?;
        let miner = decode_asset_lockup(&mut reader).ok()?;
        uncles.push(json!({
            "blockHash": block_hash.to_hex(),
            "miner": address_string(&miner),
        }));
    }
    Some(uncles)
}

/// `api.model.Transaction.fromProtocol`.
pub fn transaction(tx: &Transaction) -> Value {
    let tx_id = tx.id();
    let fixed = tx.unsigned.fixed_outputs.len();
    json!({
        "unsigned": unsigned_tx(&tx.unsigned, &tx_id),
        "scriptExecutionOk": tx.script_execution_ok,
        "contractInputs": tx.contract_inputs.iter().map(output_ref).collect::<Vec<_>>(),
        "generatedOutputs": tx
            .generated_outputs
            .iter()
            .enumerate()
            .map(|(index, output)| generated_output(output, &tx_id, index_of(fixed + index)))
            .collect::<Vec<_>>(),
        "inputSignatures": tx.input_signatures.iter().map(|sig| hex(sig)).collect::<Vec<_>>(),
        "scriptSignatures": tx.script_signatures.iter().map(|sig| hex(sig)).collect::<Vec<_>>(),
    })
}

/// `UnsignedTx.fromProtocol`; `scriptOpt` is omitted when absent.
pub fn unsigned_tx(unsigned: &UnsignedTransaction, tx_id: &Hash) -> Value {
    let mut value = json!({
        "txId": tx_id.to_hex(),
        "version": unsigned.version,
        "networkId": unsigned.network_id,
    });
    if let Some(script) = &unsigned.script {
        value["scriptOpt"] = Value::String(hex(&serialize(script)));
    }
    value["gasAmount"] = json!(unsigned.gas_amount);
    value["gasPrice"] = json!(unsigned.gas_price.to_string());
    value["inputs"] = unsigned.inputs.iter().map(asset_input).collect();
    value["fixedOutputs"] = unsigned
        .fixed_outputs
        .iter()
        .enumerate()
        .map(|(index, output)| fixed_output(output, tx_id, index_of(index)))
        .collect();
    value
}

fn index_of(index: usize) -> i32 {
    // Output counts are bounded by the decoded input size.
    i32::try_from(index).unwrap_or(i32::MAX)
}

fn output_ref(output_ref: &OutputRef) -> Value {
    json!({ "hint": output_ref.hint, "key": output_ref.key.to_hex() })
}

fn asset_input(input: &TxInput) -> Value {
    json!({
        "outputRef": output_ref(&input.output_ref),
        "unlockScript": hex(&serialize(&input.unlock_script)),
    })
}

fn tokens(tokens: &Tokens) -> Value {
    tokens
        .iter()
        .map(|(id, amount)| json!({ "id": id.to_hex(), "amount": amount.to_string() }))
        .collect()
}

/// `FixedAssetOutput.fromProtocol`.
fn fixed_output(output: &AssetOutput, tx_id: &Hash, index: i32) -> Value {
    json!({
        "hint": output_hint(&output.lockup_script),
        "key": output_key(tx_id, index).to_hex(),
        "attoAlphAmount": output.amount.to_string(),
        "address": address_string(&output.lockup_script),
        "tokens": tokens(&output.tokens),
        "lockTime": output.lock_time.0,
        "message": hex(&output.additional_data),
    })
}

/// `Output.from`: generated outputs carry their type.
fn generated_output(output: &TxOutput, tx_id: &Hash, index: i32) -> Value {
    match output {
        TxOutput::Asset(asset) => {
            let mut value = json!({ "type": "AssetOutput" });
            if let (Value::Object(target), Value::Object(fields)) =
                (&mut value, fixed_output(asset, tx_id, index))
            {
                target.extend(fields);
            }
            value
        }
        TxOutput::Contract(contract) => {
            let lockup = LockupScript::P2c(contract.contract_id);
            json!({
                "type": "ContractOutput",
                "hint": output_hint(&lockup),
                "key": output_key(tx_id, index).to_hex(),
                "attoAlphAmount": contract.amount.to_string(),
                "address": address_string(&lockup),
                "tokens": tokens(&contract.tokens),
            })
        }
    }
}
