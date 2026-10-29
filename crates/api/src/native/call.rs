//! `POST /contracts/call-contract` (`ServerUtils.callContract`, running
//! `ContractRunner.run`'s script): the method is called by a script that
//! approves the input assets, with `maximalGasPerTx`, on the world state of
//! `worldStateBlockHash` or of the group's best intra-group block; nothing
//! is kept. A call the VM fails is left to the official node for its error.

use std::sync::PoisonError;
use std::sync::atomic::{AtomicU64, Ordering};

use alephium_model::address::{address_string, base58_decode};
use alephium_model::instr::{Instr, Operand, code_of};
use alephium_model::script::Method;
use alephium_model::{
    AssetOutput, Hash, I256, LockupScript, StatefulScript, TimeStamp, U256, Val, deserialize,
};
use alephium_state::WorldState;
use alephium_vm::gas::MAXIMAL_GAS_PER_TX;
use alephium_vm::{BlockEnv, TxEnv, run_script};
use serde_json::{Value, json};

use super::Context;
use super::contract::{contract_json, val_json};

/// `nonCoinbaseMinGasPrice`: 100 nanoALPH.
const MIN_GAS_PRICE: u64 = 100_000_000_000;

fn instr(name: &str, operand: Operand) -> Option<Instr> {
    Some(Instr {
        code: code_of(name)?,
        operand,
    })
}

/// `ConstInstr.u256`.
fn u256_const(value: U256) -> Option<Instr> {
    match value.as_u64() {
        small @ 0..=5 if value <= U256::from(5u64) => {
            instr(&format!("U256Const{small}"), Operand::None)
        }
        _ => instr("U256Const", Operand::U256(value)),
    }
}

/// `Val.toConstInstr`.
fn const_instr(value: &Val) -> Option<Instr> {
    match value {
        Val::Bool(true) => instr("ConstTrue", Operand::None),
        Val::Bool(false) => instr("ConstFalse", Operand::None),
        Val::I256(value) if *value == I256::MINUS_ONE => instr("I256ConstN1", Operand::None),
        Val::I256(value) if (I256::ZERO..=I256::new(5)).contains(value) => {
            instr(&format!("I256Const{value}"), Operand::None)
        }
        Val::I256(value) => instr("I256Const", Operand::I256(*value)),
        Val::U256(value) => u256_const(*value),
        Val::ByteVec(bytes) => instr("BytesConst", Operand::Bytes(bytes.clone())),
        Val::Address(lockup) => instr("AddressConst", Operand::Address(lockup.clone())),
    }
}

/// `api.model.Val` as JSON; arrays are not accepted here.
fn parse_val(value: &Value) -> Option<Val> {
    let inner = value.get("value")?;
    Some(match value.get("type")?.as_str()? {
        "Bool" => Val::Bool(inner.as_bool()?),
        "I256" => Val::I256(inner.as_str()?.parse().ok()?),
        "U256" => Val::U256(inner.as_str()?.parse().ok()?),
        "ByteVec" => Val::ByteVec(alephium_model::hash::from_hex(inner.as_str()?)?),
        "Address" => Val::Address(parse_address(inner.as_str()?)?),
        _ => return None,
    })
}

fn parse_address(address: &str) -> Option<LockupScript> {
    deserialize::<LockupScript>(&base58_decode(address)?).ok()
}

fn contract_of(address: &str) -> Option<Hash> {
    match parse_address(address)? {
        LockupScript::P2c(id) => Some(id),
        _ => None,
    }
}

/// `TransactionId.random`.
fn random_tx_id() -> Hash {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    Hash::blake2b(&[&nanos.to_be_bytes()[..], &count.to_be_bytes()].concat())
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// `TestInputAsset`s, the first with the test gas fee added
/// (`inputAssetsWithGasFee`).
fn input_assets(request: &Value, gas_fee: U256) -> Option<Vec<AssetOutput>> {
    let Some(inputs) = request.get("inputAssets") else {
        return Some(Vec::new());
    };
    let mut assets = Vec::new();
    for (index, input) in inputs.as_array()?.iter().enumerate() {
        let lockup = parse_address(input.get("address")?.as_str()?)?;
        if !lockup.is_asset() {
            return None;
        }
        let asset = input.get("asset")?;
        let mut amount: U256 = asset.get("attoAlphAmount")?.as_str()?.parse().ok()?;
        if index == 0 {
            amount = amount.checked_add(gas_fee)?;
        }
        let mut tokens = Vec::new();
        if let Some(list) = asset.get("tokens") {
            for token in list.as_array()? {
                let id = Hash::from_hex(token.get("id")?.as_str()?)?;
                let amount: U256 = token.get("amount")?.as_str()?.parse().ok()?;
                tokens.push((id, amount));
            }
        }
        assets.push(AssetOutput {
            amount,
            lockup_script: lockup,
            lock_time: TimeStamp(0),
            tokens,
            additional_data: Vec::new(),
        });
    }
    Some(assets)
}

/// `ContractRunner.approveAsset` and `callExternal`.
fn runner_script(
    assets: &[AssetOutput],
    gas_fee: U256,
    contract: &Hash,
    method_index: u8,
    method: &Method,
    args: &[Val],
) -> Option<StatefulScript> {
    let mut instrs = Vec::new();
    for (index, asset) in assets.iter().enumerate() {
        let address = instr(
            "AddressConst",
            Operand::Address(asset.lockup_script.clone()),
        )?;
        let amount = if index == 0 {
            asset.amount.checked_sub(gas_fee)?
        } else {
            asset.amount
        };
        instrs.extend([
            address.clone(),
            u256_const(amount)?,
            instr("ApproveAlph", Operand::None)?,
        ]);
        for (id, amount) in &asset.tokens {
            instrs.extend([
                address.clone(),
                instr("BytesConst", Operand::Bytes(id.0.to_vec()))?,
                u256_const(*amount)?,
                instr("ApproveToken", Operand::None)?,
            ]);
        }
    }
    for arg in args {
        instrs.push(const_instr(arg)?);
    }
    instrs.extend([
        u256_const(U256::from(u32::try_from(method.args_length).ok()?))?,
        u256_const(U256::from(u32::try_from(method.return_length).ok()?))?,
        instr("BytesConst", Operand::Bytes(contract.0.to_vec()))?,
        instr("CallExternal", Operand::Byte(method_index))?,
    ]);
    Some(StatefulScript {
        methods: vec![Method {
            is_public: true,
            use_preapproved_assets: !assets.is_empty(),
            use_contract_assets: false,
            use_pay_to_contract_only: false,
            args_length: 0,
            locals_length: 0,
            return_length: method.return_length,
            instrs,
        }],
    })
}

pub(super) fn call_contract(body: &[u8], context: &Context<'_>) -> Option<Vec<u8>> {
    let request: Value = serde_json::from_slice(body).ok()?;
    Some(call(&request, context)?.to_string().into_bytes())
}

pub(super) fn call(request: &Value, context: &Context<'_>) -> Option<Value> {
    if request.get("callerAddress").is_some() {
        return None;
    }
    let groups = context.groups;
    let group = u32::try_from(request.get("group")?.as_u64()?).ok()?;
    let contract = contract_of(request.get("address")?.as_str()?)?;
    if group >= groups || u32::from(contract.0[31]) != group {
        return None;
    }
    let method_index = u8::try_from(request.get("methodIndex")?.as_u64()?).ok()?;
    let args: Vec<Val> = match request.get("args") {
        Some(args) => args
            .as_array()?
            .iter()
            .map(parse_val)
            .collect::<Option<_>>()?,
        None => Vec::new(),
    };
    let mut interested: Vec<Hash> = match request.get("interestedContracts") {
        Some(list) => list
            .as_array()?
            .iter()
            .map(|address| contract_of(address.as_str()?))
            .collect::<Option<_>>()?,
        None => Vec::new(),
    };
    interested.push(contract);
    let tx_id = match request.get("txId") {
        Some(id) => Hash::from_hex(id.as_str()?)?,
        None => random_tx_id(),
    };
    let (block_hash, block_time) = {
        let store = context
            .chain?
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let store = store.as_ref()?;
        let block_hash = match request.get("worldStateBlockHash") {
            Some(hash) => Hash::from_hex(hash.as_str()?)?,
            None => {
                if !store.weights_known(group, group) {
                    return None;
                }
                store.best_deps(group, group).ok()?[(groups - 1 + group) as usize]
            }
        };
        (block_hash, store.get(&block_hash)?.block.header.timestamp.0)
    };
    let states = context.state?;
    let roots = states.roots(&block_hash).ok()??;
    let mut world = WorldState::new(&roots, states);

    let gas_price = U256::from(MIN_GAS_PRICE);
    let gas_fee = gas_price * U256::from(MAXIMAL_GAS_PER_TX);
    let assets = input_assets(request, gas_fee)?;
    let method = {
        let stored =
            alephium_vm::contract::StoredState::decode(&world.contracts.get(&contract.0).ok()??)
                .ok()?;
        let code_hash = match stored {
            alephium_vm::contract::StoredState::Mutable { immutable_hash, .. } => {
                let bytes = world.immutable(&immutable_hash).ok()??;
                alephium_vm::contract::ImmutableState::decode(bytes.get(1..)?)
                    .ok()?
                    .code_hash
            }
            alephium_vm::contract::StoredState::Legacy { .. } => return None,
        };
        let code = world.immutable(&code_hash).ok()??;
        alephium_vm::contract::Code::decode(code.get(1..)?)
            .ok()?
            .method(usize::from(method_index))
            .ok()?
    };
    if usize::try_from(method.args_length).ok()? != args.len() {
        return None;
    }
    let script = runner_script(&assets, gas_fee, &contract, method_index, &method, &args)?;
    let tx = TxEnv {
        tx_id,
        prev_outputs: assets,
        fixed_outputs: Vec::new(),
        gas_price,
        gas_amount: MAXIMAL_GAS_PER_TX,
        signatures: Vec::new(),
    };
    let block = BlockEnv {
        group,
        groups,
        timestamp: TimeStamp(now_millis()),
        // `maxMiningTarget`: 2^219 - 1.
        target: Some((U256::ONE << 219u32) - U256::ONE),
        block_hash,
    };
    let run = run_script(&mut world, &block, &tx, &script, MAXIMAL_GAS_PER_TX, true).ok()?;

    let contracts: Vec<Value> = interested
        .iter()
        .map(|id| contract_json(&world, id))
        .collect::<Option<_>>()?;
    let tx_inputs: Vec<Value> = run
        .execution
        .contract_input_ids
        .iter()
        .map(|id| json!(address_string(&LockupScript::P2c(*id))))
        .collect();
    let tx_outputs: Vec<Value> = run
        .execution
        .generated_outputs
        .iter()
        .enumerate()
        .map(|(index, output)| {
            crate::json::generated_output(output, &tx_id, i32::try_from(index).unwrap_or(i32::MAX))
        })
        .collect();
    // `getNewLogs`: the events of each contract together, contracts in the
    // order they first logged.
    let mut order: Vec<Hash> = Vec::new();
    for event in &run.execution.events {
        if !order.contains(&event.contract_id) {
            order.push(event.contract_id);
        }
    }
    let mut events = Vec::new();
    for contract_id in order {
        for event in run
            .execution
            .events
            .iter()
            .filter(|event| event.contract_id == contract_id)
        {
            let Some(Val::I256(index)) = event.fields.first() else {
                continue;
            };
            let Ok(index) = i8::try_from(*index) else {
                continue;
            };
            events.push(json!({
                "blockHash": block_hash.to_hex(),
                "timestamp": block_time,
                "contractAddress": address_string(&LockupScript::P2c(contract_id)),
                "eventIndex": index,
                "fields": event.fields[1..].iter().map(val_json).collect::<Vec<_>>(),
            }));
        }
    }
    Some(json!({
        "type": "CallContractSucceeded",
        "returns": run.returns.iter().map(val_json).collect::<Vec<_>>(),
        "gasUsed": MAXIMAL_GAS_PER_TX - run.gas_remaining,
        "contracts": contracts,
        "txInputs": tx_inputs,
        "txOutputs": tx_outputs,
        "events": events,
        "debugMessages": [],
    }))
}
