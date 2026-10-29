//! `StatefulVM.runTxScript` for the Danube hard fork: frames, the
//! `StatefulContext` of one transaction, and every instruction.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

use alephium_model::address::{lockup_group, output_hint, output_key};
use alephium_model::instr::{Instr, Operand, code_of};
use alephium_model::script::Method;
use alephium_model::{
    AssetOutput, Codec, ContractOutput, Hash, I256, LockupScript, OutputRef, StatefulScript,
    TimeStamp, Transaction, TxOutput, U256, Val, deserialize, serialize,
};
use alephium_state::WorldState;
use alephium_state::trie::NodeSource;

use crate::balances::{
    BalanceState, Balances, DUST_UTXO_AMOUNT, MAX_TOKEN_PER_CONTRACT_UTXO,
    MINIMAL_ALPH_IN_CONTRACT, MutBalances, PerLockup, SharedBalanceState,
};
use crate::contract::{
    CODE_TAG, Code, ContractObj, IMMUTABLE_STATE_TAG, ImmutableState, StoredState,
};
use crate::{Fail, VmResult, arith, fail};

/// What the block gives a script (`BlockEnv`).
#[derive(Debug, Clone)]
pub struct BlockEnv {
    pub group: u32,
    pub groups: u32,
    pub timestamp: TimeStamp,
    /// `None` for a target of `2^256`, which `BlockTarget` cannot push.
    pub target: Option<U256>,
    pub block_hash: Hash,
}

/// What a transaction gives its script (`TxEnv`): the spent outputs, the
/// fixed outputs and the gas it pays for, and its script signatures.
#[derive(Debug, Clone)]
pub struct TxEnv {
    pub tx_id: Hash,
    pub prev_outputs: Vec<AssetOutput>,
    pub fixed_outputs: Vec<AssetOutput>,
    pub gas_price: U256,
    pub gas_amount: u64,
    pub signatures: Vec<[u8; 64]>,
}

impl TxEnv {
    /// `TxEnv.Default` of a transaction spending `prev_outputs`.
    #[must_use]
    pub fn of(tx: &Transaction, prev_outputs: &[AssetOutput]) -> Self {
        Self {
            tx_id: tx.id(),
            prev_outputs: prev_outputs.to_vec(),
            fixed_outputs: tx.unsigned.fixed_outputs.clone(),
            gas_price: tx.unsigned.gas_price,
            gas_amount: u64::try_from(tx.unsigned.gas_amount).unwrap_or(0),
            signatures: tx.script_signatures.clone(),
        }
    }

    /// `gasFeeUnsafe`.
    #[must_use]
    pub fn gas_fee(&self) -> U256 {
        U256::from(self.gas_amount).saturating_mul(self.gas_price)
    }
}

/// A script run: the main function's returns, what the script did, and the
/// gas left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub returns: Vec<Val>,
    pub execution: Execution,
    pub gas_remaining: u64,
}

/// A contract event (`writeLog`); `system` for creation and destruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub contract_id: Hash,
    pub fields: Vec<Val>,
    pub system: bool,
}

/// `TxScriptExecution` without gas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    pub contract_inputs: Vec<OutputRef>,
    pub generated_outputs: Vec<TxOutput>,
    pub events: Vec<Event>,
    /// `(parent, child)` contracts created as sub-contracts.
    pub sub_contracts: Vec<(Hash, Hash)>,
}

enum AssetStatus {
    InUsing(Balances),
    Flushed,
}

#[derive(Clone)]
enum Obj {
    Script(Rc<Vec<Rc<Method>>>),
    Contract(Rc<RefCell<ContractObj>>),
}

impl Obj {
    fn contract_id(&self) -> Option<Hash> {
        match self {
            Self::Script(_) => None,
            Self::Contract(obj) => Some(obj.borrow().id),
        }
    }

    fn method(&self, index: usize) -> VmResult<Rc<Method>> {
        match self {
            Self::Script(methods) => methods
                .get(index)
                .cloned()
                .ok_or_else(|| Fail(format!("invalid script method {index}"))),
            Self::Contract(obj) => obj.borrow().code.method(index),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Main,
    Local,
    External,
}

struct Frame {
    pc: usize,
    obj: Obj,
    method: Rc<Method>,
    locals: Vec<Val>,
    stack: Vec<Val>,
    balance: Option<SharedBalanceState>,
    kind: Kind,
}

struct Ctx<'w, 'a, S: NodeSource> {
    world: &'w mut WorldState<'a, S>,
    block: &'w BlockEnv,
    tx: &'w TxEnv,
    tx_id: Hash,
    prev_outputs: &'w [AssetOutput],
    signatures: VecDeque<[u8; 64]>,
    gas_fee: U256,
    tx_caller_balance: Option<SharedBalanceState>,
    output_balances: MutBalances,
    generated_outputs: Vec<TxOutput>,
    pool: HashMap<Hash, Rc<RefCell<ContractObj>>>,
    block_list: HashSet<Hash>,
    asset_status: Vec<(Hash, AssetStatus)>,
    asset_used: HashSet<(Hash, usize)>,
    contract_inputs: Vec<(OutputRef, ContractOutput)>,
    gas_fee_paid: U256,
    events: Vec<Event>,
    sub_contracts: Vec<(Hash, Hash)>,
    /// Gas left (`gasRemaining`).
    gas: u64,
}

/// Runs the script of `tx`, whose inputs spend `prev_outputs`, on `world`
/// (`StatefulVM.runTxScript`). The block says the script succeeded, so gas
/// is not limited: any failure here is a divergence from the official VM.
pub fn run_tx_script<S: NodeSource>(
    world: &mut WorldState<'_, S>,
    block: &BlockEnv,
    tx: &Transaction,
    prev_outputs: &[AssetOutput],
    script: &StatefulScript,
) -> VmResult<Execution> {
    let env = TxEnv::of(tx, prev_outputs);
    run_script(world, block, &env, script, u64::MAX, false).map(|run| run.execution)
}

/// `StatefulVM.runTxScript`, or with `with_outputs` its `...WithOutputs`
/// variant that keeps the main function's returns, with `gas` to spend.
pub fn run_script<S: NodeSource>(
    world: &mut WorldState<'_, S>,
    block: &BlockEnv,
    tx: &TxEnv,
    script: &StatefulScript,
    gas: u64,
    with_outputs: bool,
) -> VmResult<Run> {
    let gas_fee = tx.gas_fee();
    let mut ctx = Ctx {
        world,
        block,
        tx,
        tx_id: tx.tx_id,
        prev_outputs: &tx.prev_outputs,
        signatures: tx.signatures.iter().copied().collect(),
        gas_fee,
        tx_caller_balance: None,
        output_balances: MutBalances::default(),
        generated_outputs: Vec::new(),
        pool: HashMap::new(),
        block_list: HashSet::new(),
        asset_status: Vec::new(),
        asset_used: HashSet::new(),
        contract_inputs: Vec::new(),
        gas_fee_paid: U256::ZERO,
        events: Vec::new(),
        sub_contracts: Vec::new(),
        gas,
    };
    let methods: Rc<Vec<Rc<Method>>> =
        Rc::new(script.methods.iter().cloned().map(Rc::new).collect());
    let entry = methods
        .first()
        .cloned()
        .ok_or_else(|| Fail("empty script".to_owned()))?;
    if !entry.is_public {
        return fail("private entry method");
    }
    let balance = if entry.use_preapproved_assets {
        let mut balances = MutBalances::from_io(&tx.prev_outputs, &tx.fixed_outputs)
            .ok_or_else(|| Fail("invalid balances".to_owned()))?;
        let payer = tx
            .prev_outputs
            .first()
            .ok_or_else(|| Fail("no input".to_owned()))?
            .lockup_script
            .clone();
        balances
            .sub_alph(&payer, ctx.gas_fee)
            .ok_or_else(|| Fail("unable to pay the gas fee".to_owned()))?;
        let state = Rc::new(RefCell::new(BalanceState {
            remaining: std::mem::take(&mut balances),
            approved: MutBalances::default(),
        }));
        ctx.tx_caller_balance = Some(Rc::clone(&state));
        Some(state)
    } else {
        None
    };
    let mut vm = Vm {
        ctx,
        frames: Vec::new(),
        returns: with_outputs.then(Vec::new),
    };
    let frame = vm.build_frame(Obj::Script(methods), entry, Vec::new(), balance, Kind::Main)?;
    vm.frames.push(frame);
    vm.run()?;
    let ctx = vm.ctx;
    Ok(Run {
        returns: vm.returns.unwrap_or_default(),
        gas_remaining: ctx.gas,
        execution: Execution {
            contract_inputs: ctx.contract_inputs.iter().map(|(r, _)| *r).collect(),
            generated_outputs: ctx.generated_outputs,
            events: ctx.events,
            sub_contracts: ctx.sub_contracts,
        },
    })
}

struct Vm<'w, 'a, S: NodeSource> {
    ctx: Ctx<'w, 'a, S>,
    frames: Vec<Frame>,
    /// The main function's returns, kept when running with outputs.
    returns: Option<Vec<Val>>,
}

fn bool_of(value: Val) -> VmResult<bool> {
    match value {
        Val::Bool(value) => Ok(value),
        other => fail(format!("expected Bool, got {other:?}")),
    }
}

fn u256_of(value: Val) -> VmResult<U256> {
    match value {
        Val::U256(value) => Ok(value),
        other => fail(format!("expected U256, got {other:?}")),
    }
}

fn i256_of(value: Val) -> VmResult<I256> {
    match value {
        Val::I256(value) => Ok(value),
        other => fail(format!("expected I256, got {other:?}")),
    }
}

fn bytes_of(value: Val) -> VmResult<Vec<u8>> {
    match value {
        Val::ByteVec(bytes) => Ok(bytes),
        other => fail(format!("expected ByteVec, got {other:?}")),
    }
}

fn address_of(value: Val) -> VmResult<LockupScript> {
    match value {
        Val::Address(lockup) => Ok(lockup),
        other => fail(format!("expected Address, got {other:?}")),
    }
}

fn hash_of(bytes: &[u8]) -> VmResult<Hash> {
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Fail(format!("expected 32 bytes, got {}", bytes.len())))?;
    Ok(Hash(array))
}

/// `U256.toInt`: a value of at most 31 bits.
fn small_int(value: U256) -> Option<usize> {
    (value < U256::from(1u64 << 31)).then(|| value.as_usize())
}

fn p2c(id: Hash) -> LockupScript {
    LockupScript::P2c(id)
}

/// `ContractId.lemanUnsafe`: the last byte is the group.
fn with_group(hash: Hash, group: u32) -> Hash {
    let mut bytes = hash.0;
    bytes[31] = u8::try_from(group).unwrap_or(0);
    Hash(bytes)
}

fn instr(name: &str, operand: Operand) -> VmResult<Instr> {
    Ok(Instr {
        code: code_of(name).ok_or_else(|| Fail(format!("no instruction {name}")))?,
        operand,
    })
}

/// `CreateMapEntry.genContract`.
fn map_entry_code(imm_fields: usize, mut_fields: usize) -> VmResult<Code> {
    if imm_fields == 0 {
        return fail("a map entry needs its parent field");
    }
    let parent = Operand::Byte(
        u8::try_from(imm_fields - 1).map_err(|_| Fail("too many fields".to_owned()))?,
    );
    let guard = |rest: Vec<Instr>| -> VmResult<Vec<Instr>> {
        let mut instrs = vec![
            instr("CallerContractId", Operand::None)?,
            instr("LoadImmField", parent.clone())?,
            instr("ByteVecEq", Operand::None)?,
            instr("Assert", Operand::None)?,
        ];
        instrs.extend(rest);
        Ok(instrs)
    };
    let method = |args: i32, returns: i32, contract_assets: bool, instrs: Vec<Instr>| Method {
        is_public: true,
        use_preapproved_assets: false,
        use_contract_assets: contract_assets,
        use_pay_to_contract_only: false,
        args_length: args,
        locals_length: args,
        return_length: returns,
        instrs,
    };
    let methods = vec![
        method(
            1,
            1,
            false,
            guard(vec![
                instr("LoadLocal", Operand::Byte(0))?,
                instr("LoadImmFieldByIndex", Operand::None)?,
            ])?,
        ),
        method(
            1,
            1,
            false,
            guard(vec![
                instr("LoadLocal", Operand::Byte(0))?,
                instr("LoadMutFieldByIndex", Operand::None)?,
            ])?,
        ),
        method(
            2,
            0,
            false,
            guard(vec![
                instr("LoadLocal", Operand::Byte(0))?,
                instr("LoadLocal", Operand::Byte(1))?,
                instr("StoreMutFieldByIndex", Operand::None)?,
            ])?,
        ),
        method(
            1,
            0,
            true,
            guard(vec![
                instr("LoadLocal", Operand::Byte(0))?,
                instr("DestroySelf", Operand::None)?,
            ])?,
        ),
    ];
    let fields =
        i32::try_from(imm_fields + mut_fields).map_err(|_| Fail("too many fields".to_owned()))?;
    Code::from_methods(fields, &methods)
}

/// `TxOutput.from`: one output per token with the dust amount, the rest of
/// the ALPH in a last output.
fn asset_outputs(
    amount: U256,
    tokens: &[(Hash, U256)],
    lockup: &LockupScript,
    lock_time: TimeStamp,
) -> Option<Vec<TxOutput>> {
    let mut outputs: Vec<TxOutput> = tokens
        .iter()
        .map(|token| {
            TxOutput::Asset(AssetOutput {
                amount: DUST_UTXO_AMOUNT,
                lockup_script: lockup.clone(),
                lock_time,
                tokens: vec![*token],
                additional_data: Vec::new(),
            })
        })
        .collect();
    let token_dust = DUST_UTXO_AMOUNT * U256::from(tokens.len() as u64);
    if amount == token_dust {
        Some(outputs)
    } else if amount >= token_dust + DUST_UTXO_AMOUNT {
        outputs.push(TxOutput::Asset(AssetOutput {
            amount: amount - token_dust,
            lockup_script: lockup.clone(),
            lock_time,
            tokens: Vec::new(),
            additional_data: Vec::new(),
        }));
        Some(outputs)
    } else {
        None
    }
}

/// `MutBalancesPerLockup.toTxOutputLeman`.
fn to_outputs(
    balances: &PerLockup,
    lockup: &LockupScript,
    lock_time: TimeStamp,
) -> VmResult<Vec<TxOutput>> {
    let tokens = balances.token_vector();
    if balances.alph == U256::ZERO {
        return if tokens.is_empty() {
            Ok(Vec::new())
        } else {
            fail("tokens without ALPH")
        };
    }
    match lockup {
        LockupScript::P2c(id) => {
            if balances.alph < MINIMAL_ALPH_IN_CONTRACT {
                fail("contract output below the minimal deposit")
            } else if tokens.len() > MAX_TOKEN_PER_CONTRACT_UTXO {
                fail("too many tokens in a contract output")
            } else {
                Ok(vec![TxOutput::Contract(ContractOutput {
                    amount: balances.alph,
                    contract_id: *id,
                    tokens,
                })])
            }
        }
        asset => asset_outputs(balances.alph, &tokens, asset, lock_time)
            .ok_or_else(|| Fail("invalid output balances".to_owned())),
    }
}

impl<S: NodeSource> Ctx<'_, '_, S> {
    /// `chargeGas`.
    fn charge(&mut self, amount: u64) -> VmResult<()> {
        self.gas = self
            .gas
            .checked_sub(amount)
            .ok_or_else(|| Fail("out of gas".to_owned()))?;
        Ok(())
    }

    fn next_output_index(&self) -> usize {
        self.tx.fixed_outputs.len() + self.generated_outputs.len()
    }

    fn output_ref(&self, output: &TxOutput, index: usize) -> VmResult<OutputRef> {
        let hint = match output {
            TxOutput::Asset(asset) => output_hint(&asset.lockup_script),
            TxOutput::Contract(contract) => output_hint(&p2c(contract.contract_id)),
        };
        Ok(OutputRef {
            hint,
            key: output_key(
                &self.tx_id,
                i32::try_from(index).map_err(|_| Fail("output index".to_owned()))?,
            ),
        })
    }

    fn is_tx_caller(&self, lockup: &LockupScript) -> bool {
        self.prev_outputs
            .iter()
            .any(|output| &output.lockup_script == lockup)
    }

    fn unique_input_address(&mut self) -> VmResult<LockupScript> {
        self.charge(crate::gas::unique_address(self.prev_outputs.len()))?;
        let first = self
            .prev_outputs
            .first()
            .ok_or_else(|| Fail("no tx input".to_owned()))?;
        if self
            .prev_outputs
            .iter()
            .all(|output| output.lockup_script == first.lockup_script)
        {
            Ok(first.lockup_script.clone())
        } else {
            fail("tx input addresses are not identical")
        }
    }

    fn first_input_address(&self) -> VmResult<LockupScript> {
        Ok(self
            .prev_outputs
            .first()
            .ok_or_else(|| Fail("no tx input".to_owned()))?
            .lockup_script
            .clone())
    }

    fn contract_state(&self, id: &Hash) -> VmResult<Option<StoredState>> {
        self.world
            .contracts
            .get(&id.0)
            .map_err(Fail)?
            .map(|bytes| StoredState::decode(&bytes))
            .transpose()
    }

    fn put_contract_state(&mut self, id: &Hash, state: &StoredState) -> VmResult<()> {
        self.world
            .contracts
            .put(&id.0, &state.encode())
            .map_err(Fail)
    }

    fn immutable(&self, hash: &Hash, tag: u8) -> VmResult<Vec<u8>> {
        let bytes = self
            .world
            .immutable(hash)
            .map_err(Fail)?
            .ok_or_else(|| Fail(format!("missing immutable entry {}", hash.to_hex())))?;
        match bytes.split_first() {
            Some((first, rest)) if *first == tag => Ok(rest.to_vec()),
            _ => fail(format!(
                "immutable entry {} of the wrong kind",
                hash.to_hex()
            )),
        }
    }

    /// `WorldState.getContractObj`.
    fn load_from_world(&self, id: &Hash) -> VmResult<ContractObj> {
        let state = self
            .contract_state(id)?
            .ok_or_else(|| Fail(format!("no contract {}", id.to_hex())))?;
        match state {
            StoredState::Mutable {
                mut_fields,
                immutable_hash,
                ..
            } => {
                let immutable =
                    ImmutableState::decode(&self.immutable(&immutable_hash, IMMUTABLE_STATE_TAG)?)?;
                let code = Code::decode(&self.immutable(&immutable.code_hash, CODE_TAG)?)?;
                Ok(ContractObj {
                    id: *id,
                    code: Rc::new(code),
                    initial_state_hash: immutable.initial_state_hash,
                    imm_fields: immutable.imm_fields,
                    initial_mut_fields: mut_fields.clone(),
                    mut_fields,
                })
            }
            StoredState::Legacy {
                code_hash,
                initial_state_hash,
                mut_fields,
                ..
            } => {
                let record = self
                    .world
                    .code
                    .get(&code_hash.0)
                    .map_err(Fail)?
                    .ok_or_else(|| Fail(format!("missing code {}", code_hash.to_hex())))?;
                // `CodeRecord`: the code, then its reference count.
                let code_bytes =
                    &record[..record.len().saturating_sub(code_record_count_len(&record)?)];
                let code = Code::decode(code_bytes)?;
                Ok(ContractObj {
                    id: *id,
                    code: Rc::new(code),
                    initial_state_hash,
                    imm_fields: Vec::new(),
                    initial_mut_fields: mut_fields.clone(),
                    mut_fields,
                })
            }
        }
    }

    /// `ContractPool.loadContractObj`.
    fn load_contract(&mut self, id: &Hash) -> VmResult<Rc<RefCell<ContractObj>>> {
        if let Some(obj) = self.pool.get(id) {
            return Ok(Rc::clone(obj));
        }
        if self.block_list.contains(id) {
            return fail("contract load disallowed");
        }
        let loaded = self.load_from_world(id)?;
        let size = loaded
            .imm_fields
            .iter()
            .map(crate::gas::estimate)
            .sum::<usize>()
            + loaded
                .mut_fields
                .iter()
                .map(crate::gas::estimate)
                .sum::<usize>()
            + loaded.code.methods_bytes_len();
        self.charge(crate::gas::contract_load(size))?;
        let obj = Rc::new(RefCell::new(loaded));
        self.pool.insert(*id, Rc::clone(&obj));
        Ok(obj)
    }

    fn asset_status(&self, id: &Hash) -> Option<&AssetStatus> {
        self.asset_status
            .iter()
            .find(|(held, _)| held == id)
            .map(|(_, status)| status)
    }

    fn set_asset_status(&mut self, id: Hash, status: AssetStatus) {
        match self.asset_status.iter_mut().find(|(held, _)| *held == id) {
            Some(entry) => entry.1 = status,
            None => self.asset_status.push((id, status)),
        }
    }

    /// `loadContractAssets`.
    fn load_contract_assets(&mut self, id: &Hash) -> VmResult<Balances> {
        self.charge(crate::gas::TX_INPUT_BASE)?;
        let state = self
            .contract_state(id)?
            .ok_or_else(|| Fail(format!("no contract {}", id.to_hex())))?;
        let output_ref = *state.output_ref();
        let output = self
            .world
            .outputs
            .get(&serialize(&output_ref))
            .map_err(Fail)?
            .ok_or_else(|| Fail(format!("missing contract output of {}", id.to_hex())))?;
        let TxOutput::Contract(output) =
            deserialize::<TxOutput>(&output).map_err(|error| Fail(error.to_string()))?
        else {
            return fail("a contract output ref holds an asset output");
        };
        let mut balances = PerLockup::alph(output.amount);
        for (token, amount) in &output.tokens {
            balances.tokens.insert(*token, *amount);
        }
        let balances = balances.shared();
        self.contract_inputs.push((output_ref, output));
        match self.asset_status(id) {
            None => self.set_asset_status(*id, AssetStatus::InUsing(Rc::clone(&balances))),
            Some(_) => return fail("contract asset already in use or flushed"),
        }
        Ok(balances)
    }

    /// `useContractAssets` since Rhone.
    fn use_contract_assets(&mut self, id: &Hash, method_index: usize) -> VmResult<Balances> {
        if !self.asset_used.insert((*id, method_index)) {
            return fail("function reentrancy");
        }
        match self.asset_status(id) {
            Some(AssetStatus::InUsing(balances)) => Ok(Rc::clone(balances)),
            Some(AssetStatus::Flushed) => fail("contract asset already flushed"),
            None => self.load_contract_assets(id),
        }
    }

    fn prepare_pay_to_contract_only(&mut self, id: &Hash) -> VmResult<()> {
        match self.asset_status(id) {
            None => self.load_contract_assets(id).map(|_| ()),
            Some(AssetStatus::InUsing(_)) => Ok(()),
            Some(AssetStatus::Flushed) => fail("contract asset already flushed"),
        }
    }

    fn mark_flushed(&mut self, id: &Hash) -> VmResult<()> {
        match self.asset_status(id) {
            Some(AssetStatus::InUsing(_)) => {
                self.set_asset_status(*id, AssetStatus::Flushed);
                Ok(())
            }
            Some(AssetStatus::Flushed) => fail("contract asset already flushed"),
            None => fail("contract asset unloaded"),
        }
    }

    /// `generateOutput`.
    fn generate_output(&mut self, output: TxOutput) -> VmResult<()> {
        match &output {
            TxOutput::Asset(_) => {
                self.generated_outputs.push(output);
                self.charge(crate::gas::TX_OUTPUT_BASE)
            }
            TxOutput::Contract(contract) => {
                let id = contract.contract_id;
                let input_index = self
                    .contract_inputs
                    .iter()
                    .position(|(_, input)| input.contract_id == id)
                    .ok_or_else(|| Fail("contract asset unloaded".to_owned()))?;
                let generated_index = self.generated_outputs.iter().position(
                    |held| matches!(held, TxOutput::Contract(held) if held.contract_id == id),
                );
                match generated_index {
                    None => {
                        if self.contract_inputs[input_index].1 == *contract {
                            self.contract_inputs.remove(input_index);
                            self.mark_flushed(&id)
                        } else {
                            let index = self.next_output_index();
                            self.update_contract_asset(&id, &output, index)?;
                            self.generated_outputs.push(output);
                            Ok(())
                        }
                    }
                    Some(generated_index) => {
                        let index = self.tx.fixed_outputs.len() + generated_index;
                        self.contract_inputs.remove(input_index);
                        self.update_contract_asset(&id, &output, index)?;
                        self.generated_outputs[generated_index] = output;
                        Ok(())
                    }
                }
            }
        }
    }

    /// `updateContractAsset`: the new output and the contract's reference
    /// to it.
    fn update_contract_asset(
        &mut self,
        id: &Hash,
        output: &TxOutput,
        index: usize,
    ) -> VmResult<()> {
        self.charge(crate::gas::TX_OUTPUT_BASE)?;
        self.mark_flushed(id)?;
        let output_ref = self.output_ref(output, index)?;
        let mut state = self
            .contract_state(id)?
            .ok_or_else(|| Fail(format!("no contract {}", id.to_hex())))?;
        self.world.put_output(&output_ref, output).map_err(Fail)?;
        state.set_output_ref(output_ref);
        self.put_contract_state(id, &state)
    }

    /// `coverExtraAlphAmount`: from the first input's remaining, then
    /// approved, balances of the transaction caller.
    fn cover_extra_alph(&mut self, target: &Balances, to_cover: U256) -> VmResult<()> {
        let caller_state = self
            .tx_caller_balance
            .clone()
            .ok_or_else(|| Fail("tx caller balance not available".to_owned()))?;
        let caller = self.first_input_address()?;
        let mut left = to_cover;
        for approved in [false, true] {
            if left == U256::ZERO {
                break;
            }
            let state = caller_state.borrow();
            let source = if approved {
                &state.approved
            } else {
                &state.remaining
            };
            if let Some(balances) = source.get(&caller) {
                let able = left.min(balances.borrow().alph);
                balances
                    .borrow_mut()
                    .sub_alph(able)
                    .ok_or_else(|| Fail("balance".to_owned()))?;
                target
                    .borrow_mut()
                    .add_alph(able)
                    .ok_or_else(|| Fail("balance".to_owned()))?;
                left -= able;
            }
        }
        if left == U256::ZERO {
            Ok(())
        } else {
            fail("insufficient funds for the UTXO dust amount")
        }
    }

    /// `createContract`.
    fn create_contract(
        &mut self,
        id: Hash,
        code: &Rc<Code>,
        imm_fields: Vec<Val>,
        initial_balances: &Balances,
        mut_fields: Vec<Val>,
        issuance: Option<(U256, Option<LockupScript>)>,
    ) -> VmResult<()> {
        if let Some((amount, transfer_to)) = issuance {
            match transfer_to {
                Some(to) => self.output_balances.add_token(&to, id, amount),
                None => initial_balances.borrow_mut().add_token(id, amount),
            }
            .ok_or_else(|| Fail("token issuance overflow".to_owned()))?;
        }
        let output = TxOutput::Contract(ContractOutput {
            amount: initial_balances.borrow().alph,
            contract_id: id,
            tokens: initial_balances.borrow().token_vector(),
        });
        let output_ref = self.output_ref(&output, self.next_output_index())?;
        if self.contract_state(&id)?.is_some() {
            return fail("contract already exists");
        }
        let fields = imm_fields.len() + mut_fields.len();
        if i32::try_from(fields).ok() != Some(code.field_length) {
            return fail("invalid field length");
        }
        // `createContractLemanUnsafe`.
        let immutable = ImmutableState {
            code_hash: code.hash,
            initial_state_hash: code.initial_state_hash(&imm_fields, &mut_fields),
            imm_fields,
        };
        let immutable_hash = immutable.hash();
        self.world.put_output(&output_ref, &output).map_err(Fail)?;
        self.put_contract_state(
            &id,
            &StoredState::Mutable {
                mut_fields,
                output_ref,
                immutable_hash,
            },
        )?;
        self.world.put_immutable(
            immutable_hash,
            [&[IMMUTABLE_STATE_TAG][..], &immutable.encode()].concat(),
        );
        self.world
            .put_immutable(code.hash, [&[CODE_TAG][..], &code.encode()].concat());
        self.generated_outputs.push(output);
        Ok(())
    }

    /// `removeContract`.
    fn remove_contract(&mut self, id: &Hash) -> VmResult<()> {
        let state = self
            .contract_state(id)?
            .ok_or_else(|| Fail(format!("no contract {}", id.to_hex())))?;
        if let StoredState::Legacy { code_hash, .. } = state {
            // `removeContractCodeDeprecated`: the code record's count.
            let record = self
                .world
                .code
                .get(&code_hash.0)
                .map_err(Fail)?
                .ok_or_else(|| Fail("missing code record".to_owned()))?;
            let count_len = code_record_count_len(&record)?;
            let (code, count) = record.split_at(record.len() - count_len);
            let count = deserialize::<i32>(count).map_err(|error| Fail(error.to_string()))?;
            if count > 1 {
                let mut updated = code.to_vec();
                (count - 1).encode(&mut updated);
                self.world.code.put(&code_hash.0, &updated).map_err(Fail)?;
            } else {
                self.world.code.remove(&code_hash.0).map_err(Fail)?;
            }
        }
        self.world.contracts.remove(&id.0).map_err(Fail)?;
        self.mark_flushed(id)?;
        self.pool.remove(id);
        Ok(())
    }

    fn write_log(&mut self, contract_id: Option<Hash>, fields: Vec<Val>, system: bool) {
        if let Some(contract_id) = contract_id {
            self.events.push(Event {
                contract_id,
                fields,
                system,
            });
        }
    }
}

/// The byte length of the reference count closing a `CodeRecord`: the code
/// is decoded to find where it ends.
fn code_record_count_len(record: &[u8]) -> VmResult<usize> {
    let mut reader = alephium_model::Reader::new(record);
    reader.int().map_err(|error| Fail(error.to_string()))?;
    let ends = Vec::<i32>::decode(&mut reader).map_err(|error| Fail(error.to_string()))?;
    let length = usize::try_from(ends.last().copied().unwrap_or(0))
        .map_err(|_| Fail("offset".to_owned()))?;
    reader
        .take(length)
        .map_err(|error| Fail(error.to_string()))?;
    Ok(reader.remaining())
}

impl<S: NodeSource> Vm<'_, '_, S> {
    fn top(&mut self) -> VmResult<&mut Frame> {
        self.frames
            .last_mut()
            .ok_or_else(|| Fail("no frame".to_owned()))
    }

    fn pop(&mut self) -> VmResult<Val> {
        self.top()?
            .stack
            .pop()
            .ok_or_else(|| Fail("stack underflow".to_owned()))
    }

    /// `Stack.pop(n)`: the top `n` values, oldest first.
    fn pop_n(&mut self, n: usize) -> VmResult<Vec<Val>> {
        let stack = &mut self.top()?.stack;
        if stack.len() < n {
            return fail("stack underflow");
        }
        Ok(stack.split_off(stack.len() - n))
    }

    fn push(&mut self, value: Val) -> VmResult<()> {
        self.top()?.stack.push(value);
        Ok(())
    }

    fn pop_bool(&mut self) -> VmResult<bool> {
        bool_of(self.pop()?)
    }

    fn pop_u256(&mut self) -> VmResult<U256> {
        u256_of(self.pop()?)
    }

    fn pop_i256(&mut self) -> VmResult<I256> {
        i256_of(self.pop()?)
    }

    fn pop_bytes(&mut self) -> VmResult<Vec<u8>> {
        bytes_of(self.pop()?)
    }

    fn pop_address(&mut self) -> VmResult<LockupScript> {
        address_of(self.pop()?)
    }

    fn pop_asset_address(&mut self) -> VmResult<LockupScript> {
        let address = self.pop_address()?;
        if address.is_asset() {
            Ok(address)
        } else {
            fail("expected an asset address")
        }
    }

    fn pop_contract_id(&mut self) -> VmResult<Hash> {
        hash_of(&self.pop_bytes()?)
    }

    fn pop_token_id(&mut self) -> VmResult<Hash> {
        hash_of(&self.pop_bytes()?)
    }

    /// `popFields`: serialized `AVector[Val]`.
    fn pop_fields(&mut self) -> VmResult<Vec<Val>> {
        let bytes = self.pop_bytes()?;
        deserialize::<Vec<Val>>(&bytes).map_err(|error| Fail(format!("fields: {error}")))
    }

    /// An index into fields or locals given on the stack (`popIndex`).
    fn pop_index(&mut self) -> VmResult<usize> {
        let value = self.pop_u256()?;
        small_int(value)
            .filter(|index| *index <= 0xff)
            .ok_or_else(|| Fail("invalid index".to_owned()))
    }

    fn balance(&mut self) -> VmResult<SharedBalanceState> {
        self.top()?
            .balance
            .clone()
            .ok_or_else(|| Fail("no balance available".to_owned()))
    }

    fn contract(&mut self) -> VmResult<Rc<RefCell<ContractObj>>> {
        match &self.top()?.obj {
            Obj::Contract(obj) => Ok(Rc::clone(obj)),
            Obj::Script(_) => fail("expected a contract"),
        }
    }

    fn self_id(&mut self) -> VmResult<Hash> {
        Ok(self.contract()?.borrow().id)
    }

    /// `Frame.build`: the arguments come off the caller's stack.
    fn build_frame(
        &mut self,
        obj: Obj,
        method: Rc<Method>,
        args: Vec<Val>,
        balance: Option<SharedBalanceState>,
        kind: Kind,
    ) -> VmResult<Frame> {
        let args_length =
            usize::try_from(method.args_length).map_err(|_| Fail("args".to_owned()))?;
        let locals_length =
            usize::try_from(method.locals_length).map_err(|_| Fail("locals".to_owned()))?;
        if args.len() != args_length || locals_length < args_length {
            return fail("invalid method arguments");
        }
        let mut locals = args;
        locals.resize(locals_length, Val::Bool(false));
        Ok(Frame {
            pc: 0,
            obj,
            method,
            locals,
            stack: Vec::new(),
            balance,
            kind,
        })
    }

    /// `getNewFrameBalancesStateSinceLeman`.
    fn new_frame_balance(
        &mut self,
        obj: &Obj,
        method: &Method,
        method_index: usize,
    ) -> VmResult<Option<SharedBalanceState>> {
        let (state, use_approved) = if method.use_preapproved_assets {
            let current = self.balance()?;
            let new = current.borrow_mut().use_approved();
            (new, true)
        } else if method.use_contract_assets || method.use_pay_to_contract_only {
            (BalanceState::default(), false)
        } else {
            return Ok(None);
        };
        let state = Rc::new(RefCell::new(state));
        let without_contract = if use_approved {
            Some(Rc::clone(&state))
        } else {
            None
        };
        let Some(id) = obj.contract_id() else {
            return Ok(without_contract);
        };
        if method.use_contract_assets {
            let balances = self.ctx.use_contract_assets(&id, method_index)?;
            state
                .borrow_mut()
                .remaining
                .add(&p2c(id), &balances)
                .ok_or_else(|| Fail("balance overflow".to_owned()))?;
            Ok(Some(state))
        } else if method.use_pay_to_contract_only {
            self.ctx.prepare_pay_to_contract_only(&id)?;
            Ok(Some(state))
        } else {
            Ok(without_contract)
        }
    }

    fn call_local(&mut self, index: usize) -> VmResult<()> {
        self.ctx.charge(crate::gas::CALL)?;
        let obj = self.top()?.obj.clone();
        let method = obj.method(index)?;
        let balance = self.new_frame_balance(&obj, &method, index)?;
        let args = self.pop_n(usize::try_from(method.args_length).unwrap_or(0))?;
        let frame = self.build_frame(obj, method, args, balance, Kind::Local)?;
        self.frames.push(frame);
        Ok(())
    }

    /// `callExternal` / `callExternalBySelector`.
    fn call_external(&mut self, by: Result<usize, i32>) -> VmResult<()> {
        self.ctx.charge(crate::gas::CALL)?;
        let id = self.pop_contract_id()?;
        let contract = self.ctx.load_contract(&id)?;
        let index = match by {
            Ok(index) => index,
            Err(selector) => {
                let (index, searched) = contract.borrow().code.method_by_selector(selector)?;
                self.ctx.charge(searched * crate::gas::MID)?;
                index
            }
        };
        let method = contract.borrow().code.method(index)?;
        // `checkLength`: the return length, then the argument count.
        for (expected, what) in [
            (method.return_length, "return"),
            (method.args_length, "argument"),
        ] {
            let given = small_int(self.pop_u256()?);
            if given != usize::try_from(expected).ok() {
                return fail(format!("external call {what} length mismatch"));
            }
        }
        if !method.is_public {
            return fail("external call of a private method");
        }
        let obj = Obj::Contract(contract);
        let balance = self.new_frame_balance(&obj, &method, index)?;
        let args = self.pop_n(usize::try_from(method.args_length).unwrap_or(0))?;
        let frame = self.build_frame(obj, method, args, balance, Kind::External)?;
        self.frames.push(frame);
        Ok(())
    }

    /// `Return`: the values go to the caller, and a contract returning to
    /// the script hands the caller's outputs back (`chainCallerOutputs`).
    fn do_return(&mut self) -> VmResult<()> {
        let length = usize::try_from(self.top()?.method.return_length).unwrap_or(0);
        let values = self.pop_n(length)?;
        let frame = self.top()?;
        frame.pc = frame.method.instrs.len();
        let kind = frame.kind;
        let callee_is_contract = matches!(frame.obj, Obj::Contract(_));
        let depth = self.frames.len();
        match kind {
            Kind::Main => match &mut self.returns {
                Some(returns) => {
                    *returns = values;
                    Ok(())
                }
                None if values.is_empty() => Ok(()),
                None => fail("non-empty return of the main function"),
            },
            Kind::Local => {
                self.frames[depth - 2].stack.extend(values);
                Ok(())
            }
            Kind::External => {
                let caller = &mut self.frames[depth - 2];
                caller.stack.extend(values);
                if matches!(caller.obj, Obj::Script(_))
                    && callee_is_contract
                    && let Some(balance) = caller.balance.clone()
                {
                    self.chain_caller_outputs(&balance)?;
                }
                Ok(())
            }
        }
    }

    /// `chainCallerOutputs`.
    fn chain_caller_outputs(&mut self, balance: &SharedBalanceState) -> VmResult<()> {
        let mut callers: Vec<LockupScript> = Vec::new();
        for output in self.ctx.prev_outputs {
            if !callers.contains(&output.lockup_script) {
                callers.push(output.lockup_script.clone());
            }
        }
        for caller in callers {
            if let Some(outputs) = self.ctx.output_balances.take_for_chained_input(&caller) {
                balance
                    .borrow_mut()
                    .remaining
                    .add(&caller, &outputs)
                    .ok_or_else(|| Fail("chain caller outputs failed".to_owned()))?;
            }
        }
        Ok(())
    }

    /// `mergeBack` in Danube.
    fn merge_back(
        &mut self,
        previous: Option<&SharedBalanceState>,
        current: &MutBalances,
        use_contract_assets: bool,
        contract_id: Option<Hash>,
        approved: bool,
    ) -> VmResult<()> {
        for (lockup, balances) in &current.all {
            let keep = !approved && contract_id.is_some_and(|id| *lockup == p2c(id));
            let depth = balances.borrow().scope_depth;
            let result = if depth <= 0 {
                if keep {
                    Some(())
                } else {
                    self.ctx.output_balances.add(lockup, balances)
                }
            } else if keep && use_contract_assets {
                Some(())
            } else {
                match previous {
                    Some(previous) => previous.borrow_mut().remaining.add(lockup, balances),
                    None => self.ctx.output_balances.add(lockup, balances),
                }
            };
            result.ok_or_else(|| Fail("balance error when switching back".to_owned()))?;
        }
        Ok(())
    }

    /// `postFrame`.
    fn post_frame(&mut self) -> VmResult<()> {
        let current = self
            .frames
            .pop()
            .ok_or_else(|| Fail("no frame".to_owned()))?;
        if self.frames.is_empty() {
            return self.complete_last_frame();
        }
        let Some(current_balance) = current.balance else {
            return Ok(());
        };
        let previous = self.top()?.balance.clone();
        let contract_id = current.obj.contract_id();
        let use_contract_assets = current.method.use_contract_assets;
        let (remaining, approved) = {
            let state = current_balance.borrow();
            (state.remaining.clone(), state.approved.clone())
        };
        self.merge_back(
            previous.as_ref(),
            &remaining,
            use_contract_assets,
            contract_id,
            false,
        )?;
        self.merge_back(
            previous.as_ref(),
            &approved,
            use_contract_assets,
            contract_id,
            true,
        )
    }

    /// `completeLastFrame`: contract fields, then the balances
    /// (`cleanBalancesDanube`), then spent contract outputs.
    fn complete_last_frame(&mut self) -> VmResult<()> {
        let updated: Vec<(Hash, Vec<Val>)> = self
            .ctx
            .pool
            .iter()
            .filter(|(_, obj)| obj.borrow().is_updated())
            .map(|(id, obj)| (*id, obj.borrow().mut_fields.clone()))
            .collect();
        for (id, fields) in updated {
            self.ctx
                .charge(crate::gas::CONTRACT_STATE_UPDATE_BASE + crate::gas::fields(&fields))?;
            let mut state = self
                .ctx
                .contract_state(&id)?
                .ok_or_else(|| Fail(format!("no contract {}", id.to_hex())))?;
            state.set_mut_fields(fields);
            self.ctx.put_contract_state(&id, &state)?;
        }
        // `reimburseGas`.
        if self.ctx.gas_fee_paid > U256::ZERO
            && let Some(first) = self.ctx.prev_outputs.first()
        {
            let payer = first.lockup_script.clone();
            let paid = self.ctx.gas_fee_paid;
            self.ctx
                .output_balances
                .add_alph(&payer, paid)
                .ok_or_else(|| Fail("balance overflow".to_owned()))?;
        }
        // `outputRemainingContractAssetsForRhone`.
        let in_using: Vec<(Hash, Balances)> = self
            .ctx
            .asset_status
            .iter()
            .filter_map(|(id, status)| match status {
                AssetStatus::InUsing(balances) => Some((*id, Rc::clone(balances))),
                AssetStatus::Flushed => None,
            })
            .collect();
        for (id, balances) in in_using {
            self.ctx
                .output_balances
                .add(&p2c(id), &balances)
                .ok_or_else(|| Fail("balance overflow".to_owned()))?;
        }
        // `coverUtxoMinimalAmountsDanube`.
        let all = self.ctx.output_balances.all.clone();
        for (lockup, balances) in &all {
            if self.ctx.is_tx_caller(lockup) {
                continue;
            }
            let to_cover = balances.borrow().minimal_alph_to_cover(lockup);
            if let Some(to_cover) = to_cover {
                self.ctx.cover_extra_alph(balances, to_cover)?;
            }
        }
        // `flushCallerBalances`.
        if let Some(caller) = self.ctx.tx_caller_balance.clone() {
            let mut caller = caller.borrow_mut();
            self.ctx
                .output_balances
                .merge(&mut caller.approved)
                .ok_or_else(|| Fail("invalid balances".to_owned()))?;
            self.ctx
                .output_balances
                .merge(&mut caller.remaining)
                .ok_or_else(|| Fail("invalid balances".to_owned()))?;
        }
        // `outputGeneratedBalances`.
        let all = self.ctx.output_balances.all.clone();
        for (lockup, balances) in &all {
            if let LockupScript::P2c(id) = lockup
                && self.ctx.asset_status(id).is_none()
            {
                return fail("contract asset unloaded");
            }
            let outputs = to_outputs(&balances.borrow(), lockup, TimeStamp(0))?;
            for output in outputs {
                self.ctx.generate_output(output)?;
            }
        }
        // `checkAllAssetsFlushed`.
        if self
            .ctx
            .asset_status
            .iter()
            .any(|(_, status)| matches!(status, AssetStatus::InUsing(_)))
        {
            return fail("contract asset not flushed");
        }
        // `removeOutdatedContractAssets`.
        let spent: Vec<OutputRef> = self.ctx.contract_inputs.iter().map(|(r, _)| *r).collect();
        for output_ref in spent {
            self.ctx.world.remove_output(&output_ref).map_err(Fail)?;
        }
        Ok(())
    }

    fn run(&mut self) -> VmResult<()> {
        while let Some(frame) = self.frames.last() {
            let pc = frame.pc;
            let length = frame.method.instrs.len();
            if pc < length {
                let instr = frame.method.instrs[pc].clone();
                match instr.name() {
                    "CallLocal" => {
                        self.top()?.pc += 1;
                        let Operand::Byte(index) = instr.operand else {
                            return fail("CallLocal operand");
                        };
                        self.call_local(usize::from(index))?;
                    }
                    "CallExternal" => {
                        self.top()?.pc += 1;
                        let Operand::Byte(index) = instr.operand else {
                            return fail("CallExternal operand");
                        };
                        self.call_external(Ok(usize::from(index)))?;
                    }
                    "CallExternalBySelector" => {
                        self.top()?.pc += 1;
                        let Operand::Selector(selector) = instr.operand else {
                            return fail("CallExternalBySelector operand");
                        };
                        self.call_external(Err(selector))?;
                    }
                    "Return" => {
                        self.do_return()?;
                        self.post_frame()?;
                    }
                    _ => {
                        if let Some(gas) = crate::gas::simple(instr.name()) {
                            self.ctx.charge(gas)?;
                        }
                        self.step(&instr)?;
                        self.top()?.pc += 1;
                    }
                }
            } else if pc == length {
                self.do_return()?;
                self.post_frame()?;
            } else {
                return fail("pc overflow");
            }
        }
        Ok(())
    }
}

/// `Val.toByteVec`.
fn to_byte_vec(value: &Val) -> Vec<u8> {
    match value {
        Val::Bool(value) => serialize(value),
        Val::I256(value) => serialize(value),
        Val::U256(value) => serialize(value),
        Val::ByteVec(bytes) => bytes.clone(),
        Val::Address(lockup) => serialize(lockup),
    }
}

fn fixed_size_bytes(value: U256, size: usize) -> Option<Vec<u8>> {
    let bytes = value.to_be_bytes();
    let leading = bytes.iter().take_while(|byte| **byte == 0).count();
    (32 - leading <= size).then(|| bytes[32 - size..].to_vec())
}

impl<S: NodeSource> Vm<'_, '_, S> {
    fn binary_u256(&mut self, op: impl Fn(U256, U256) -> Option<Val>) -> VmResult<()> {
        let y = self.pop_u256()?;
        let x = self.pop_u256()?;
        let out = op(x, y).ok_or_else(|| Fail("arithmetic error".to_owned()))?;
        self.push(out)
    }

    fn binary_i256(&mut self, op: impl Fn(I256, I256) -> Option<Val>) -> VmResult<()> {
        let y = self.pop_i256()?;
        let x = self.pop_i256()?;
        let out = op(x, y).ok_or_else(|| Fail("arithmetic error".to_owned()))?;
        self.push(out)
    }

    /// `NumericBitAnd` and the others: both U256, or both I256.
    fn bitwise(
        &mut self,
        u: impl Fn(U256, U256) -> U256,
        i: impl Fn(I256, I256) -> I256,
    ) -> VmResult<()> {
        let y = self.pop()?;
        let x = self.pop()?;
        let out = match (x, y) {
            (Val::U256(x), Val::U256(y)) => Val::U256(u(x, y)),
            (Val::I256(x), Val::I256(y)) => Val::I256(i(x, y)),
            _ => return fail("bitwise operands of different types"),
        };
        self.push(out)
    }

    fn shift(&mut self, left: bool) -> VmResult<()> {
        let n = self.pop_u256()?;
        let x = self.pop()?;
        let out = match x {
            Val::U256(x) => Val::U256(
                if left {
                    arith::u256_shl(x, n)
                } else {
                    Some(arith::u256_shr(x, n))
                }
                .ok_or_else(|| Fail("shift overflow".to_owned()))?,
            ),
            Val::I256(x) => Val::I256(
                if left {
                    arith::i256_shl(x, n)
                } else {
                    Some(arith::i256_shr(x, n))
                }
                .ok_or_else(|| Fail("shift overflow".to_owned()))?,
            ),
            _ => return fail("shift of a non-number"),
        };
        self.push(out)
    }

    fn jump(&mut self, offset: i32) -> VmResult<()> {
        let frame = self.top()?;
        let target = i64::try_from(frame.pc).unwrap_or(i64::MAX) + i64::from(offset);
        let length = i64::try_from(frame.method.instrs.len()).unwrap_or(i64::MAX);
        if (0..length).contains(&target) {
            frame.pc = usize::try_from(target).unwrap_or(0);
            Ok(())
        } else {
            fail("invalid instruction offset")
        }
    }

    fn pop_signature(&mut self) -> VmResult<[u8; 64]> {
        self.ctx
            .signatures
            .pop_front()
            .ok_or_else(|| Fail("no signature left".to_owned()))
    }

    /// Transfers, approvals and the other balance moves.
    fn use_alph(&mut self, from: &LockupScript, amount: U256) -> VmResult<()> {
        self.balance()?
            .borrow()
            .remaining
            .sub_alph(from, amount)
            .ok_or_else(|| Fail("not enough approved ALPH".to_owned()))
    }

    fn use_token(&mut self, from: &LockupScript, token: Hash, amount: U256) -> VmResult<()> {
        self.balance()?
            .borrow()
            .remaining
            .sub_token(from, token, amount)
            .ok_or_else(|| Fail("not enough approved tokens".to_owned()))
    }

    fn transfer_alph(
        &mut self,
        from: &LockupScript,
        to: &LockupScript,
        amount: U256,
    ) -> VmResult<()> {
        if amount == U256::ZERO {
            return Ok(());
        }
        self.use_alph(from, amount)?;
        self.ctx
            .output_balances
            .add_alph(to, amount)
            .ok_or_else(|| Fail("balance overflow".to_owned()))
    }

    fn transfer_token(
        &mut self,
        token: Hash,
        from: &LockupScript,
        to: &LockupScript,
        amount: U256,
    ) -> VmResult<()> {
        if token == Hash::ZERO {
            return self.transfer_alph(from, to, amount);
        }
        if amount == U256::ZERO {
            return Ok(());
        }
        self.use_token(from, token, amount)?;
        self.ctx
            .output_balances
            .add_token(to, token, amount)
            .ok_or_else(|| Fail("balance overflow".to_owned()))
    }

    /// `checkNonRecursive` over the caller trace from frame `index` down.
    fn in_caller_trace(&self, id: &Hash, from: usize) -> bool {
        self.frames[..=from]
            .iter()
            .rev()
            .map_while(|frame| frame.obj.contract_id())
            .any(|frame_id| frame_id == *id)
    }

    /// `getToAddressFromStack`: a contract receives only when it is in the
    /// caller trace.
    fn pop_to_address(&mut self) -> VmResult<LockupScript> {
        let to = self.pop_address()?;
        if let LockupScript::P2c(id) = &to
            && !self.in_caller_trace(id, self.frames.len() - 1)
        {
            return fail("pay to a contract not in the caller trace");
        }
        Ok(to)
    }

    /// `getCallAddress` of frame `index`.
    fn call_address(&mut self, index: usize) -> VmResult<LockupScript> {
        match self.frames[index].obj.contract_id() {
            Some(id) => Ok(p2c(id)),
            None => self.ctx.unique_input_address(),
        }
    }

    fn caller_index(&self) -> VmResult<usize> {
        self.frames
            .len()
            .checked_sub(2)
            .ok_or_else(|| Fail("no caller".to_owned()))
    }

    /// `getExternalCallerAddress`.
    fn external_caller_address(&mut self) -> VmResult<LockupScript> {
        let top = self.frames.len() - 1;
        let Some(id) = self.frames[top].obj.contract_id() else {
            return self.ctx.unique_input_address();
        };
        let mut index = top;
        loop {
            index = index
                .checked_sub(1)
                .ok_or_else(|| Fail("external caller not available".to_owned()))?;
            if self.frames[index].obj.contract_id() != Some(id) {
                return self.call_address(index);
            }
        }
    }

    /// `ContractFactory.__runWith`.
    fn create(&mut self, name: &str, operand: &Operand) -> VmResult<()> {
        let sub_contract = name.contains("SubContract") || name == "CreateMapEntry";
        let copy_create = name.starts_with("CopyCreate");
        let issuance = if name.ends_with("AndTransferToken") {
            let to = self.pop_asset_address()?;
            let amount = self.pop_u256()?;
            Some((amount, Some(to)))
        } else if name.ends_with("WithToken") {
            Some((self.pop_u256()?, None))
        } else {
            None
        };
        let (imm_fields, mut_fields, code) = if let Operand::BytePair(imm, mutable) = operand {
            let mut_fields = self.pop_n(usize::from(*mutable))?;
            let imm_fields = self.pop_n(usize::from(*imm))?;
            self.ctx
                .charge(crate::gas::fields(imm_fields.iter().chain(&mut_fields)))?;
            let code = map_entry_code(usize::from(*imm), usize::from(*mutable))?;
            self.ctx
                .charge(crate::gas::code_size(code.encode().len()))?;
            (imm_fields, mut_fields, Rc::new(code))
        } else {
            let mut_fields = self.pop_fields()?;
            let imm_fields = self.pop_fields()?;
            self.ctx
                .charge(crate::gas::fields(imm_fields.iter().chain(&mut_fields)))?;
            let code = if copy_create {
                let id = self.pop_contract_id()?;
                let obj = self.ctx.load_contract(&id)?;
                Rc::clone(&obj.borrow().code)
            } else {
                let bytes = self.pop_bytes()?;
                let code = Code::decode(&bytes)?;
                self.ctx.charge(crate::gas::code_size(bytes.len()))?;
                if code.field_length > 0xff || code.methods_length() == 0 {
                    return fail("invalid contract code");
                }
                for index in 0..code.methods_length() {
                    let method = code.method(index)?;
                    if !method.is_valid()
                        || (method.use_contract_assets && method.use_pay_to_contract_only)
                    {
                        return fail("invalid contract method");
                    }
                }
                Rc::new(code)
            };
            (imm_fields, mut_fields, code)
        };
        let group = self.ctx.block.group;
        let (id, parent) = if sub_contract {
            let parent = self.self_id()?;
            let path = self.pop_bytes()?;
            let pre_image = [&parent.0[..], &path].concat();
            self.ctx
                .charge(crate::gas::hash(pre_image.len()) + crate::gas::hash(32))?;
            let id = with_group(Hash::blake2b(&Hash::blake2b(&pre_image).0), group);
            (id, Some(parent))
        } else {
            let index = u32::try_from(self.ctx.next_output_index())
                .map_err(|_| Fail("index".to_owned()))?;
            let pre_image = [&self.ctx.tx_id.0[..], &index.to_be_bytes()].concat();
            (with_group(Hash::blake2b(&pre_image), group), None)
        };
        // `Frame.createContract`.
        if id == Hash::ZERO {
            return fail("zero contract id");
        }
        let approved_any = match &self.top()?.balance {
            Some(balance) => !balance.borrow().approved.all.is_empty(),
            None => false,
        };
        let initial = if approved_any {
            self.balance()?
                .borrow_mut()
                .approved
                .take_sum()
                .ok_or_else(|| Fail("invalid balances".to_owned()))?
        } else {
            PerLockup::default()
        };
        let initial = initial.shared();
        let alph = initial.borrow().alph;
        if alph < MINIMAL_ALPH_IN_CONTRACT {
            self.ctx
                .cover_extra_alph(&initial, MINIMAL_ALPH_IN_CONTRACT - alph)?;
        }
        self.ctx.create_contract(
            id,
            &code,
            imm_fields.clone(),
            &initial,
            mut_fields,
            issuance,
        )?;
        let interface = match imm_fields.last() {
            Some(Val::ByteVec(bytes)) if bytes.starts_with(&CREATE_INTERFACE_PREFIX) => {
                Val::ByteVec(bytes[CREATE_INTERFACE_PREFIX.len()..].to_vec())
            }
            _ => Val::ByteVec(Vec::new()),
        };
        self.ctx.write_log(
            Some(event_contract_id(CREATE_EVENT_INDEX, group)),
            vec![
                Val::I256(I256::from(CREATE_EVENT_INDEX)),
                Val::Address(p2c(id)),
                parent.map_or(Val::ByteVec(Vec::new()), |parent| Val::Address(p2c(parent))),
                interface,
            ],
            true,
        );
        if let Some(parent) = parent {
            self.ctx.sub_contracts.push((parent, id));
        }
        if name != "CreateMapEntry" {
            self.push(Val::ByteVec(id.0.to_vec()))?;
        }
        Ok(())
    }

    /// `DestroySelf`.
    fn destroy(&mut self) -> VmResult<()> {
        let mut refund = self.pop_address()?;
        if refund == p2c(Hash::ZERO) {
            refund = self.ctx.first_input_address()?;
        }
        let top = self.frames.len() - 1;
        if let LockupScript::P2c(to) = &refund
            && !self.in_caller_trace(to, top)
        {
            return fail("refund to a contract not in the caller trace");
        }
        let id = self.self_id()?;
        let caller = self.caller_index()?;
        if self.in_caller_trace(&id, caller) {
            return fail("contract destruction called from itself");
        }
        let assets = self
            .balance()?
            .borrow_mut()
            .remaining
            .take(&p2c(id))
            .ok_or_else(|| Fail("invalid balances".to_owned()))?;
        self.ctx
            .output_balances
            .add(&refund, &assets)
            .ok_or_else(|| Fail("invalid balances".to_owned()))?;
        self.ctx.remove_contract(&id)?;
        let group = self.ctx.block.group;
        self.ctx.write_log(
            Some(event_contract_id(DESTROY_EVENT_INDEX, group)),
            vec![
                Val::I256(I256::from(DESTROY_EVENT_INDEX)),
                Val::Address(p2c(id)),
            ],
            true,
        );
        self.do_return()?;
        self.top()?.pc -= 1;
        Ok(())
    }

    /// `MigrateSimple` / `MigrateWithFields`.
    fn migrate(&mut self, with_fields: bool) -> VmResult<()> {
        let fields = if with_fields {
            let mut_fields = self.pop_fields()?;
            let imm_fields = self.pop_fields()?;
            Some((imm_fields, mut_fields))
        } else {
            None
        };
        let bytes = self.pop_bytes()?;
        self.ctx.charge(crate::gas::code_size(bytes.len()))?;
        let code = Code::decode(&bytes)?;
        if code.field_length > 0xff || code.methods_length() == 0 {
            return fail("invalid contract code");
        }
        for index in 0..code.methods_length() {
            let method = code.method(index)?;
            if !method.is_valid() || (method.use_contract_assets && method.use_pay_to_contract_only)
            {
                return fail("invalid contract method");
            }
        }
        let obj = self.contract()?;
        let id = obj.borrow().id;
        let caller = self.caller_index()?;
        if self.in_caller_trace(&id, caller) {
            return fail("recursive call in migration");
        }
        let (imm_fields, mut_fields) = fields.unwrap_or_else(|| {
            let obj = obj.borrow();
            (obj.imm_fields.clone(), obj.mut_fields.clone())
        });
        if i32::try_from(imm_fields.len() + mut_fields.len()).ok() != Some(code.field_length) {
            return fail("invalid field length");
        }
        self.ctx
            .charge(crate::gas::fields(imm_fields.iter().chain(&mut_fields)))?;
        let Some(StoredState::Mutable {
            output_ref,
            immutable_hash,
            ..
        }) = self.ctx.contract_state(&id)?
        else {
            return fail("unable to migrate a pre-Leman contract");
        };
        let old =
            ImmutableState::decode(&self.ctx.immutable(&immutable_hash, IMMUTABLE_STATE_TAG)?)?;
        let immutable = ImmutableState {
            code_hash: code.hash,
            initial_state_hash: old.initial_state_hash,
            imm_fields,
        };
        let new_hash = immutable.hash();
        self.ctx.put_contract_state(
            &id,
            &StoredState::Mutable {
                mut_fields,
                output_ref,
                immutable_hash: new_hash,
            },
        )?;
        self.ctx.world.put_immutable(
            new_hash,
            [&[IMMUTABLE_STATE_TAG][..], &immutable.encode()].concat(),
        );
        self.ctx
            .world
            .put_immutable(code.hash, [&[CODE_TAG][..], &code.encode()].concat());
        self.ctx.block_list.insert(id);
        self.ctx.pool.remove(&id);
        self.do_return()?;
        self.top()?.pc -= 1;
        Ok(())
    }
}

/// `createContractEventIndex` and `destroyContractEventIndex`.
const CREATE_EVENT_INDEX: i64 = -1;
const DESTROY_EVENT_INDEX: i64 = -2;
/// `createContractInterfaceIdPrefix`.
const CREATE_INTERFACE_PREFIX: [u8; 4] = *b"ALPH";

/// `specialContractId`: zeros, then the event index as a byte, then the
/// group (`createContractEventId`, `destroyContractEventId`).
fn event_contract_id(index: i64, group: u32) -> Hash {
    let mut bytes = [0u8; 32];
    bytes[30] = index.to_be_bytes()[7];
    with_group(Hash(bytes), group)
}

impl<S: NodeSource> Vm<'_, '_, S> {
    /// Every instruction but calls and `Return`.
    #[allow(clippy::too_many_lines)]
    fn step(&mut self, instr: &Instr) -> VmResult<()> {
        let name = instr.name();
        match (name, &instr.operand) {
            ("ConstTrue", _) => self.push(Val::Bool(true)),
            ("ConstFalse", _) => self.push(Val::Bool(false)),
            ("I256Const0", _) => self.push(Val::I256(I256::ZERO)),
            ("I256Const1", _) => self.push(Val::I256(I256::ONE)),
            ("I256Const2", _) => self.push(Val::I256(I256::new(2))),
            ("I256Const3", _) => self.push(Val::I256(I256::new(3))),
            ("I256Const4", _) => self.push(Val::I256(I256::new(4))),
            ("I256Const5", _) => self.push(Val::I256(I256::new(5))),
            ("I256ConstN1", _) => self.push(Val::I256(I256::MINUS_ONE)),
            ("U256Const0", _) => self.push(Val::U256(U256::ZERO)),
            ("U256Const1", _) => self.push(Val::U256(U256::ONE)),
            ("U256Const2", _) => self.push(Val::U256(U256::new(2))),
            ("U256Const3", _) => self.push(Val::U256(U256::new(3))),
            ("U256Const4", _) => self.push(Val::U256(U256::new(4))),
            ("U256Const5", _) => self.push(Val::U256(U256::new(5))),
            ("I256Const", Operand::I256(value)) => self.push(Val::I256(*value)),
            ("U256Const", Operand::U256(value)) => self.push(Val::U256(*value)),
            ("BytesConst", Operand::Bytes(bytes)) => self.push(Val::ByteVec(bytes.clone())),
            ("AddressConst", Operand::Address(lockup)) => self.push(Val::Address(lockup.clone())),
            ("LoadLocal", Operand::Byte(index)) => {
                let value = self
                    .top()?
                    .locals
                    .get(usize::from(*index))
                    .cloned()
                    .ok_or_else(|| Fail("invalid local".to_owned()))?;
                self.push(value)
            }
            ("StoreLocal", Operand::Byte(index)) => {
                let value = self.pop()?;
                let slot = self
                    .top()?
                    .locals
                    .get_mut(usize::from(*index))
                    .ok_or_else(|| Fail("invalid local".to_owned()))?;
                *slot = value;
                Ok(())
            }
            ("LoadLocalByIndex", _) => {
                let index = self.pop_index()?;
                let value = self
                    .top()?
                    .locals
                    .get(index)
                    .cloned()
                    .ok_or_else(|| Fail("invalid local".to_owned()))?;
                self.push(value)
            }
            ("StoreLocalByIndex", _) => {
                let index = self.pop_index()?;
                let value = self.pop()?;
                let slot = self
                    .top()?
                    .locals
                    .get_mut(index)
                    .ok_or_else(|| Fail("invalid local".to_owned()))?;
                *slot = value;
                Ok(())
            }
            ("LoadImmField", Operand::Byte(index)) => {
                let value = self.imm_field(usize::from(*index))?;
                self.push(value)
            }
            ("LoadImmFieldByIndex", _) => {
                let index = self.pop_index()?;
                let value = self.imm_field(index)?;
                self.push(value)
            }
            ("LoadMutField", Operand::Byte(index)) => {
                let value = self.mut_field(usize::from(*index))?;
                self.push(value)
            }
            ("LoadMutFieldByIndex", _) => {
                let index = self.pop_index()?;
                let value = self.mut_field(index)?;
                self.push(value)
            }
            ("StoreMutField", Operand::Byte(index)) => {
                let value = self.pop()?;
                self.set_mut_field(usize::from(*index), value)
            }
            ("StoreMutFieldByIndex", _) => {
                let index = self.pop_index()?;
                let value = self.pop()?;
                self.set_mut_field(index, value)
            }
            ("Pop", _) => self.pop().map(|_| ()),
            ("Dup", _) => {
                let value = self
                    .top()?
                    .stack
                    .last()
                    .cloned()
                    .ok_or_else(|| Fail("stack underflow".to_owned()))?;
                self.push(value)
            }
            ("Swap", _) => {
                let stack = &mut self.top()?.stack;
                let length = stack.len();
                if length < 2 {
                    return fail("stack underflow");
                }
                stack.swap(length - 1, length - 2);
                Ok(())
            }
            ("BoolNot", _) => {
                let value = self.pop_bool()?;
                self.push(Val::Bool(!value))
            }
            ("BoolAnd" | "BoolOr" | "BoolEq" | "BoolNeq", _) => {
                let y = self.pop_bool()?;
                let x = self.pop_bool()?;
                let out = match name {
                    "BoolAnd" => x && y,
                    "BoolOr" => x || y,
                    "BoolEq" => x == y,
                    _ => x != y,
                };
                self.push(Val::Bool(out))
            }
            ("BoolToByteVec" | "I256ToByteVec" | "U256ToByteVec" | "AddressToByteVec", _) => {
                let value = self.pop()?;
                let ok = matches!(
                    (name, &value),
                    ("BoolToByteVec", Val::Bool(_))
                        | ("I256ToByteVec", Val::I256(_))
                        | ("U256ToByteVec", Val::U256(_))
                        | ("AddressToByteVec", Val::Address(_))
                );
                if !ok {
                    return fail(format!("{name} of {value:?}"));
                }
                let bytes = to_byte_vec(&value);
                self.ctx.charge(crate::gas::to_byte(bytes.len()))?;
                self.push(Val::ByteVec(bytes))
            }
            ("I256Add", _) => self.binary_i256(|x, y| x.checked_add(y).map(Val::I256)),
            ("I256Sub", _) => self.binary_i256(|x, y| x.checked_sub(y).map(Val::I256)),
            ("I256Mul", _) => self.binary_i256(|x, y| x.checked_mul(y).map(Val::I256)),
            ("I256Div", _) => self.binary_i256(|x, y| arith::i256_div(x, y).map(Val::I256)),
            ("I256Mod", _) => self.binary_i256(|x, y| arith::i256_mod(x, y).map(Val::I256)),
            ("I256RoundInfinityDiv", _) => {
                self.binary_i256(|x, y| arith::i256_round_infinity_div(x, y).map(Val::I256))
            }
            ("I256Eq", _) => self.binary_i256(|x, y| Some(Val::Bool(x == y))),
            ("I256Neq", _) => self.binary_i256(|x, y| Some(Val::Bool(x != y))),
            ("I256Lt", _) => self.binary_i256(|x, y| Some(Val::Bool(x < y))),
            ("I256Le", _) => self.binary_i256(|x, y| Some(Val::Bool(x <= y))),
            ("I256Gt", _) => self.binary_i256(|x, y| Some(Val::Bool(x > y))),
            ("I256Ge", _) => self.binary_i256(|x, y| Some(Val::Bool(x >= y))),
            ("U256Add", _) => self.binary_u256(|x, y| x.checked_add(y).map(Val::U256)),
            ("U256Sub", _) => self.binary_u256(|x, y| x.checked_sub(y).map(Val::U256)),
            ("U256Mul", _) => self.binary_u256(|x, y| x.checked_mul(y).map(Val::U256)),
            ("U256Div", _) => self.binary_u256(|x, y| x.checked_div(y).map(Val::U256)),
            ("U256Mod", _) => self.binary_u256(|x, y| x.checked_rem(y).map(Val::U256)),
            ("U256RoundInfinityDiv", _) => {
                self.binary_u256(|x, y| arith::u256_round_infinity_div(x, y).map(Val::U256))
            }
            ("U256ModAdd", _) => self.binary_u256(|x, y| Some(Val::U256(x.wrapping_add(y)))),
            ("U256ModSub", _) => self.binary_u256(|x, y| Some(Val::U256(x.wrapping_sub(y)))),
            ("U256ModMul", _) => self.binary_u256(|x, y| Some(Val::U256(x.wrapping_mul(y)))),
            ("U256Eq", _) => self.binary_u256(|x, y| Some(Val::Bool(x == y))),
            ("U256Neq", _) => self.binary_u256(|x, y| Some(Val::Bool(x != y))),
            ("U256Lt", _) => self.binary_u256(|x, y| Some(Val::Bool(x < y))),
            ("U256Le", _) => self.binary_u256(|x, y| Some(Val::Bool(x <= y))),
            ("U256Gt", _) => self.binary_u256(|x, y| Some(Val::Bool(x > y))),
            ("U256Ge", _) => self.binary_u256(|x, y| Some(Val::Bool(x >= y))),
            ("NumericBitAnd", _) => self.bitwise(|x, y| x & y, |x, y| x & y),
            ("NumericBitOr", _) => self.bitwise(|x, y| x | y, |x, y| x | y),
            ("NumericXor", _) => self.bitwise(|x, y| x ^ y, |x, y| x ^ y),
            ("NumericSHL", _) => self.shift(true),
            ("NumericSHR", _) => self.shift(false),
            ("I256Exp", _) => {
                let exp = self.pop_u256()?;
                self.ctx.charge(crate::gas::exp(byte_length(exp)))?;
                let base = self.pop_i256()?;
                let out =
                    arith::i256_pow(base, exp).ok_or_else(|| Fail("exp overflow".to_owned()))?;
                self.push(Val::I256(out))
            }
            ("U256Exp", _) => {
                let exp = self.pop_u256()?;
                self.ctx.charge(crate::gas::exp(byte_length(exp)))?;
                let base = self.pop_u256()?;
                let out =
                    arith::u256_pow(base, exp).ok_or_else(|| Fail("exp overflow".to_owned()))?;
                self.push(Val::U256(out))
            }
            ("U256ModExp", _) => {
                let exp = self.pop_u256()?;
                self.ctx.charge(crate::gas::exp(byte_length(exp)))?;
                let base = self.pop_u256()?;
                self.push(Val::U256(arith::u256_mod_pow(base, exp)))
            }
            ("MulModN" | "AddModN", _) => {
                let n = self.pop_u256()?;
                let y = self.pop_u256()?;
                let x = self.pop_u256()?;
                let out = arith::mod_n(x, y, n, name == "MulModN")
                    .ok_or_else(|| Fail("mod n".to_owned()))?;
                self.push(Val::U256(out))
            }
            ("I256ToU256", _) => {
                let value = self.pop_i256()?;
                if value < I256::ZERO {
                    return fail("negative I256 to U256");
                }
                self.push(Val::U256(value.as_u256()))
            }
            ("U256ToI256", _) => {
                let value = self.pop_u256()?;
                if value > I256::MAX.as_u256() {
                    return fail("U256 too large for I256");
                }
                self.push(Val::I256(value.as_i256()))
            }
            ("U256ToString", _) => {
                let value = self.pop_u256()?;
                let bytes = value.to_string().into_bytes();
                self.ctx.charge(crate::gas::to_byte(bytes.len()))?;
                self.push(Val::ByteVec(bytes))
            }
            ("I256ToString", _) => {
                let value = self.pop_i256()?;
                let bytes = value.to_string().into_bytes();
                self.ctx.charge(crate::gas::to_byte(bytes.len()))?;
                self.push(Val::ByteVec(bytes))
            }
            ("BoolToString", _) => {
                let value = self.pop_bool()?;
                let bytes = if value {
                    b"true".to_vec()
                } else {
                    b"false".to_vec()
                };
                self.ctx.charge(crate::gas::to_byte(bytes.len()))?;
                self.push(Val::ByteVec(bytes))
            }
            (
                "U256To1Byte" | "U256To2Byte" | "U256To4Byte" | "U256To8Byte" | "U256To16Byte"
                | "U256To32Byte",
                _,
            ) => {
                let size = byte_size(name, "U256To")?;
                let value = self.pop_u256()?;
                let bytes = fixed_size_bytes(value, size)
                    .ok_or_else(|| Fail("value too large".to_owned()))?;
                self.ctx.charge(crate::gas::to_byte(size))?;
                self.push(Val::ByteVec(bytes))
            }
            (
                "U256From1Byte" | "U256From2Byte" | "U256From4Byte" | "U256From8Byte"
                | "U256From16Byte" | "U256From32Byte",
                _,
            ) => {
                let size = byte_size(name, "U256From")?;
                let bytes = self.pop_bytes()?;
                if bytes.len() != size {
                    return fail("invalid bytes size");
                }
                self.ctx.charge(crate::gas::to_byte(size))?;
                let mut array = [0u8; 32];
                array[32 - size..].copy_from_slice(&bytes);
                self.push(Val::U256(U256::from_be_bytes(array)))
            }
            ("ByteVecEq" | "ByteVecNeq", _) => {
                let x = self.pop_bytes()?;
                let y = self.pop_bytes()?;
                self.ctx.charge(crate::gas::bytes_eq(x.len()))?;
                self.push(Val::Bool((x == y) == (name == "ByteVecEq")))
            }
            ("AddressEq" | "AddressNeq", _) => {
                let x = self.pop_address()?;
                let y = self.pop_address()?;
                self.push(Val::Bool((x == y) == (name == "AddressEq")))
            }
            ("ByteVecSize", _) => {
                let bytes = self.pop_bytes()?;
                self.push(Val::U256(U256::from(bytes.len() as u64)))
            }
            ("ByteVecConcat", _) => {
                let y = self.pop_bytes()?;
                let mut x = self.pop_bytes()?;
                x.extend(y);
                self.ctx.charge(crate::gas::bytes_concat(x.len()))?;
                self.push(Val::ByteVec(x))
            }
            ("ByteVecSlice", _) => {
                let end =
                    small_int(self.pop_u256()?).ok_or_else(|| Fail("slice end".to_owned()))?;
                let begin =
                    small_int(self.pop_u256()?).ok_or_else(|| Fail("slice begin".to_owned()))?;
                let bytes = self.pop_bytes()?;
                if begin > end || end > bytes.len() {
                    return fail("invalid slice");
                }
                self.ctx.charge(crate::gas::bytes_concat(end - begin))?;
                self.push(Val::ByteVec(bytes[begin..end].to_vec()))
            }
            ("ByteVecToAddress", _) => {
                let bytes = self.pop_bytes()?;
                self.ctx.charge(crate::gas::to_byte(bytes.len()))?;
                let lockup =
                    deserialize::<LockupScript>(&bytes).map_err(|error| Fail(error.to_string()))?;
                self.push(Val::Address(lockup))
            }
            ("ContractIdToAddress", _) => {
                let id = self.pop_contract_id()?;
                self.push(Val::Address(p2c(id)))
            }
            ("Encode", _) => {
                let n =
                    small_int(self.pop_u256()?).ok_or_else(|| Fail("encode length".to_owned()))?;
                let values = self.pop_n(n)?;
                let bytes = serialize(&values);
                self.ctx.charge(crate::gas::bytes_concat(bytes.len()))?;
                self.push(Val::ByteVec(bytes))
            }
            ("Zeros", _) => {
                let size = small_int(self.pop_u256()?)
                    .filter(|size| *size <= 4096)
                    .ok_or_else(|| Fail("zeros size".to_owned()))?;
                self.ctx.charge(crate::gas::bytes_eq(size))?;
                self.push(Val::ByteVec(vec![0; size]))
            }
            ("IsAssetAddress" | "IsContractAddress", _) => {
                let address = self.pop_address()?;
                self.push(Val::Bool(address.is_asset() == (name == "IsAssetAddress")))
            }
            ("GroupOfAddress", _) => {
                let address = self.pop_address()?;
                let group = match &address {
                    LockupScript::P2c(id) => u32::from(id.0[31]),
                    asset => lockup_group(asset, self.ctx.block.groups),
                };
                self.push(Val::U256(U256::from(group)))
            }
            ("Jump", Operand::Offset(offset)) => self.jump(*offset),
            ("IfTrue" | "IfFalse", Operand::Offset(offset)) => {
                let value = self.pop_bool()?;
                if value == (name == "IfTrue") {
                    self.jump(*offset)
                } else {
                    Ok(())
                }
            }
            ("Assert", _) => {
                if self.pop_bool()? {
                    Ok(())
                } else {
                    fail("assertion failed")
                }
            }
            ("AssertWithErrorCode", _) => {
                let code = self.pop_u256()?;
                if self.pop_bool()? {
                    Ok(())
                } else {
                    fail(format!("assertion failed with error code {code}"))
                }
            }
            ("Blake2b" | "Keccak256" | "Sha256" | "Sha3", _) => {
                let bytes = self.pop_bytes()?;
                self.ctx.charge(crate::gas::hash(bytes.len()))?;
                self.push(Val::ByteVec(crate::hashes::digest(name, &bytes)))
            }
            // The block says the script succeeded, so its signature checks
            // passed: only what they consume matters here.
            ("VerifyTxSignature", _) => {
                self.pop_bytes()?;
                self.pop_signature().map(|_| ())
            }
            ("VerifySecP256K1" | "VerifyED25519" | "VerifyBIP340Schnorr", _) => {
                self.pop_bytes()?;
                self.pop_bytes()?;
                self.pop_bytes().map(|_| ())
            }
            ("VerifySignature", _) => {
                for _ in 0..4 {
                    self.pop_bytes()?;
                }
                Ok(())
            }
            ("GetSegregatedSignature", _) => {
                let signature = self.pop_signature()?;
                self.push(Val::ByteVec(signature.to_vec()))
            }
            ("NetworkId", _) => self.push(Val::ByteVec(vec![alephium_model::network::NETWORK_ID])),
            ("BlockTimeStamp", _) => self.push(Val::U256(U256::from(self.ctx.block.timestamp.0))),
            ("BlockTarget", _) => {
                let target = self
                    .ctx
                    .block
                    .target
                    .ok_or_else(|| Fail("invalid block target".to_owned()))?;
                self.push(Val::U256(target))
            }
            ("BlockHash", _) => self.push(Val::ByteVec(self.ctx.block.block_hash.0.to_vec())),
            ("TxId", _) => self.push(Val::ByteVec(self.ctx.tx_id.0.to_vec())),
            ("TxGasPrice", _) => self.push(Val::U256(self.ctx.tx.gas_price)),
            ("TxGasAmount", _) => {
                let amount = self.ctx.tx.gas_amount;
                self.push(Val::U256(U256::from(amount)))
            }
            ("TxGasFee", _) => self.push(Val::U256(self.ctx.gas_fee)),
            ("TxInputAddressAt" | "TxInputsSize", _) => {
                if matches!(self.top()?.obj, Obj::Contract(_)) {
                    return fail("tx inputs accessed in a contract");
                }
                if name == "TxInputsSize" {
                    let size = self.ctx.prev_outputs.len() as u64;
                    return self.push(Val::U256(U256::from(size)));
                }
                let index =
                    small_int(self.pop_u256()?).ok_or_else(|| Fail("input index".to_owned()))?;
                let address = self
                    .ctx
                    .prev_outputs
                    .get(index)
                    .ok_or_else(|| Fail("invalid tx input index".to_owned()))?
                    .lockup_script
                    .clone();
                self.push(Val::Address(address))
            }
            ("VerifyAbsoluteLocktime", _) => {
                let until = self.pop_u256()?;
                if until > U256::from(self.ctx.block.timestamp.0) {
                    fail("absolute lock time not reached")
                } else {
                    Ok(())
                }
            }
            ("VerifyRelativeLocktime", _) => {
                let duration = self.pop_u256()?;
                let index =
                    small_int(self.pop_u256()?).ok_or_else(|| Fail("input index".to_owned()))?;
                let lock_time = self
                    .ctx
                    .prev_outputs
                    .get(index)
                    .ok_or_else(|| Fail("invalid tx input index".to_owned()))?
                    .lock_time;
                if lock_time.0 == 0 {
                    return fail("relative lock time needs a persisted UTXO");
                }
                let until = U256::from(lock_time.0)
                    .checked_add(duration)
                    .ok_or_else(|| Fail("lock time overflow".to_owned()))?;
                if until > U256::from(self.ctx.block.timestamp.0) {
                    fail("relative lock time not reached")
                } else {
                    Ok(())
                }
            }
            ("Log1" | "Log2" | "Log3" | "Log4" | "Log5" | "Log6" | "Log7" | "Log8" | "Log9", _) => {
                let n = usize::from(name.as_bytes()[3] - b'0');
                self.ctx.charge(crate::gas::log(n))?;
                let fields = self.pop_n(n)?;
                let id = self.top()?.obj.contract_id();
                self.ctx.write_log(id, fields, false);
                Ok(())
            }
            ("MethodSelector", _) => Ok(()),
            _ => self.step_stateful(name, &instr.operand),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn step_stateful(&mut self, name: &str, operand: &Operand) -> VmResult<()> {
        match name {
            "ApproveAlph" => {
                let amount = self.pop_u256()?;
                let address = self.pop_address()?;
                self.approve(&address, Hash::ZERO, amount)
            }
            "ApproveToken" => {
                let amount = self.pop_u256()?;
                let token = self.pop_token_id()?;
                let address = self.pop_address()?;
                self.approve(&address, token, amount)
            }
            "AlphRemaining" => {
                let address = self.pop_address()?;
                let amount = self
                    .balance()?
                    .borrow()
                    .alph_remaining(&address)
                    .unwrap_or(U256::ZERO);
                self.push(Val::U256(amount))
            }
            "TokenRemaining" => {
                let token = self.pop_token_id()?;
                let address = self.pop_address()?;
                let balance = self.balance()?;
                let amount = if token == Hash::ZERO {
                    balance.borrow().alph_remaining(&address)
                } else {
                    balance.borrow().token_remaining(&address, &token)
                };
                self.push(Val::U256(amount.unwrap_or(U256::ZERO)))
            }
            "IsPaying" => {
                let address = self.pop_address()?;
                let paying = self.balance()?.borrow().is_paying(&address);
                self.push(Val::Bool(paying))
            }
            "TransferAlph" | "TransferAlphFromSelf" | "TransferAlphToSelf" => {
                let amount = self.pop_u256()?;
                let (from, to) = self.transfer_ends(name)?;
                self.transfer_alph(&from, &to, amount)
            }
            "TransferToken" | "TransferTokenFromSelf" | "TransferTokenToSelf" => {
                let amount = self.pop_u256()?;
                let token = self.pop_token_id()?;
                let (from, to) = self.transfer_ends(name)?;
                self.transfer_token(token, &from, &to, amount)
            }
            "BurnToken" => {
                let amount = self.pop_u256()?;
                let token = self.pop_token_id()?;
                let from = self.pop_address()?;
                if token == Hash::ZERO {
                    return fail("burning ALPH is not allowed");
                }
                self.use_token(&from, token, amount)
            }
            "LockApprovedAssets" => {
                let until = self.pop_u256()?;
                let until = u64::try_from(until)
                    .ok()
                    .filter(|until| i64::try_from(*until).is_ok())
                    .ok_or_else(|| Fail("lock time overflow".to_owned()))?;
                if until <= self.ctx.block.timestamp.0 {
                    return fail("invalid lock time");
                }
                let lockup = self.pop_asset_address()?;
                let approved = self
                    .balance()?
                    .borrow_mut()
                    .approved
                    .take_sum()
                    .ok_or_else(|| Fail("no assets approved".to_owned()))?;
                let outputs = to_outputs(&approved, &lockup, TimeStamp(until))?;
                for output in outputs {
                    self.ctx.generate_output(output)?;
                }
                Ok(())
            }
            "PayGasFee" => {
                let amount = self.pop_u256()?;
                let payer = self.pop_address()?;
                let to_pay = self.ctx.gas_fee - self.ctx.gas_fee_paid;
                if to_pay < amount {
                    return fail("gas over paid");
                }
                self.use_alph(&payer, amount)?;
                self.ctx.gas_fee_paid += amount;
                Ok(())
            }
            "MinimalContractDeposit" => self.push(Val::U256(MINIMAL_ALPH_IN_CONTRACT)),
            "CreateContract"
            | "CreateContractWithToken"
            | "CreateContractAndTransferToken"
            | "CopyCreateContract"
            | "CopyCreateContractWithToken"
            | "CopyCreateContractAndTransferToken"
            | "CreateSubContract"
            | "CreateSubContractWithToken"
            | "CreateSubContractAndTransferToken"
            | "CopyCreateSubContract"
            | "CopyCreateSubContractWithToken"
            | "CopyCreateSubContractAndTransferToken"
            | "CreateMapEntry" => self.create(name, operand),
            "ContractExists" => {
                let id = self.pop_contract_id()?;
                let exists = self.ctx.contract_state(&id)?.is_some();
                self.push(Val::Bool(exists))
            }
            "DestroySelf" => self.destroy(),
            "MigrateSimple" => self.migrate(false),
            "MigrateWithFields" => self.migrate(true),
            "SelfAddress" => {
                let id = self.self_id()?;
                self.push(Val::Address(p2c(id)))
            }
            "SelfContractId" => {
                let id = self.self_id()?;
                self.push(Val::ByteVec(id.0.to_vec()))
            }
            "SubContractId" | "SubContractIdOf" => {
                let path = self.pop_bytes()?;
                let parent = if name == "SubContractId" {
                    self.self_id()?
                } else {
                    self.pop_contract_id()?
                };
                let pre_image = [&parent.0[..], &path].concat();
                self.ctx.charge(
                    crate::gas::bytes_concat(pre_image.len())
                        + crate::gas::hash(pre_image.len())
                        + crate::gas::hash(32),
                )?;
                let id = with_group(
                    Hash::blake2b(&Hash::blake2b(&pre_image).0),
                    self.ctx.block.group,
                );
                self.push(Val::ByteVec(id.0.to_vec()))
            }
            "ALPHTokenId" => self.push(Val::ByteVec(vec![0; 32])),
            "NullContractAddress" => self.push(Val::Address(p2c(Hash::ZERO))),
            "CallerContractId" => {
                let caller = self.caller_index()?;
                let id = self.frames[caller]
                    .obj
                    .contract_id()
                    .ok_or_else(|| Fail("the caller is not a contract".to_owned()))?;
                self.push(Val::ByteVec(id.0.to_vec()))
            }
            "CallerAddress" => {
                let address = match self.caller_index() {
                    Ok(caller) => self.call_address(caller)?,
                    Err(_) => self.ctx.unique_input_address()?,
                };
                self.push(Val::Address(address))
            }
            "ExternalCallerAddress" => {
                let address = self.external_caller_address()?;
                self.push(Val::Address(address))
            }
            "ExternalCallerContractId" => match self.external_caller_address()? {
                LockupScript::P2c(id) => self.push(Val::ByteVec(id.0.to_vec())),
                _ => fail("the external caller is not a contract"),
            },
            "IsCalledFromTxScript" => {
                let caller = self.caller_index()?;
                let script = matches!(self.frames[caller].obj, Obj::Script(_));
                self.push(Val::Bool(script))
            }
            "CallerInitialStateHash" | "CallerCodeHash" => {
                let caller = self.caller_index()?;
                let Obj::Contract(obj) = self.frames[caller].obj.clone() else {
                    return fail("the caller is not a contract");
                };
                let hash = if name == "CallerInitialStateHash" {
                    obj.borrow().initial_state_hash
                } else {
                    obj.borrow().code.hash
                };
                self.push(Val::ByteVec(hash.0.to_vec()))
            }
            "ContractInitialStateHash" | "ContractCodeHash" => {
                let id = self.pop_contract_id()?;
                let obj = self.ctx.load_contract(&id)?;
                let hash = if name == "ContractInitialStateHash" {
                    obj.borrow().initial_state_hash
                } else {
                    obj.borrow().code.hash
                };
                self.push(Val::ByteVec(hash.0.to_vec()))
            }
            other => fail(format!("instruction {other} is not supported")),
        }
    }

    fn approve(&mut self, from: &LockupScript, token: Hash, amount: U256) -> VmResult<()> {
        if amount == U256::ZERO {
            return Ok(());
        }
        let balance = self.balance()?;
        let mut balance = balance.borrow_mut();
        let result = if token == Hash::ZERO {
            balance.approve_alph(from, amount)
        } else {
            balance.approve_token(from, token, amount)
        };
        result.ok_or_else(|| Fail("not enough balance to approve".to_owned()))
    }

    /// The `from` and `to` of a transfer, popped as the official
    /// instructions pop them.
    fn transfer_ends(&mut self, name: &str) -> VmResult<(LockupScript, LockupScript)> {
        let (from, to) = if name.ends_with("FromSelf") {
            let to = self.pop_to_address()?;
            (p2c(self.self_id()?), to)
        } else if name.ends_with("ToSelf") {
            let to = p2c(self.self_id()?);
            (self.pop_address()?, to)
        } else {
            let to = self.pop_to_address()?;
            (self.pop_address()?, to)
        };
        if (name.ends_with("FromSelf") || name.ends_with("ToSelf")) && from == to {
            return fail("invalid self transfer");
        }
        Ok((from, to))
    }

    fn imm_field(&mut self, index: usize) -> VmResult<Val> {
        self.contract()?
            .borrow()
            .imm_fields
            .get(index)
            .cloned()
            .ok_or_else(|| Fail("invalid immutable field".to_owned()))
    }

    fn mut_field(&mut self, index: usize) -> VmResult<Val> {
        self.contract()?
            .borrow()
            .mut_fields
            .get(index)
            .cloned()
            .ok_or_else(|| Fail("invalid mutable field".to_owned()))
    }

    fn set_mut_field(&mut self, index: usize, value: Val) -> VmResult<()> {
        let contract = self.contract()?;
        let mut contract = contract.borrow_mut();
        let slot = contract
            .mut_fields
            .get_mut(index)
            .ok_or_else(|| Fail("invalid mutable field".to_owned()))?;
        if slot.type_id() != value.type_id() {
            return fail("mutable field of another type");
        }
        *slot = value;
        Ok(())
    }
}

/// `U256.byteLength`.
fn byte_length(value: U256) -> usize {
    (256 - value.leading_zeros() as usize).div_ceil(8)
}

fn byte_size(name: &str, prefix: &str) -> VmResult<usize> {
    name.strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix("Byte"))
        .and_then(|size| size.parse().ok())
        .ok_or_else(|| Fail(format!("bad instruction name {name}")))
}
