//! どこで: EVM custom precompile / 何を: Kasane専用precompile群 / なぜ: EVM tx内でIC連携intentを確定するため

use crate::hash;
use evm_db::chain_data::constants::{CHAIN_ID, MAX_LOG_DATA};
use evm_db::chain_data::receipt::LogEntry;
use evm_db::chain_data::MAX_ICP_UPDATE_REQUESTS;
use evm_db::stable_state::current_runtime_config;
use revm::{
    context::Cfg,
    context_interface::{
        journaled_state::account::JournaledAccountTr, ContextTr, JournalTr, LocalContextTr,
    },
    handler::{EthPrecompiles, PrecompileProvider},
    interpreter::{CallInputs, Gas, InstructionResult, InterpreterResult},
    primitives::{Address, Bytes, Log, B256, U256},
};
use std::boxed::Box;
#[cfg(not(target_arch = "wasm32"))]
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

// 予約レンジ方針:
// - 0x00000000000000000000000000000000ffff0001: ICRC wrapped token unwrap
// - 0x00000000000000000000000000000000ffff0002: Kasane native ICP withdrawal
// - 0x00000000000000000000000000000000ffff0003: ICP query precompile
// - 0x00000000000000000000000000000000ffff0004: ICP update intent precompile
// - 0x00000000000000000000000000000000ffff0005+: 将来拡張用の予約スロット
pub const WRAP_PRECOMPILE_ADDRESS: Address = Address::new([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0x00, 0x01,
]);
pub const NATIVE_WITHDRAW_PRECOMPILE_ADDRESS: Address = Address::new([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0x00, 0x02,
]);
pub const ICP_QUERY_PRECOMPILE_ADDRESS: Address = Address::new([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0x00, 0x03,
]);
pub const ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS: Address = Address::new([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0x00, 0x04,
]);
const MAX_FIELD_LEN: usize = 120;
const MAX_PRINCIPAL_LEN: usize = 29;
const MAX_QUERY_METHOD_LEN: usize = 64;
const MAX_ICP_UPDATE_ARG_LEN: usize = 3_997;
const MAX_ICP_QUERY_ARG_LEN: usize = MAX_ICP_UPDATE_ARG_LEN;
const _: () = assert!(
    1 + MAX_PRINCIPAL_LEN + 1 + MAX_QUERY_METHOD_LEN + 4 + MAX_ICP_UPDATE_ARG_LEN <= MAX_LOG_DATA
);
const COMPACT_UNWRAP_FORMAT_VERSION: u8 = 1;
const COMPACT_NATIVE_WITHDRAW_FORMAT_VERSION: u8 = 1;
const COMPACT_ICP_PRECOMPILE_FORMAT_VERSION: u8 = 1;
const ICP_QUERY_KIND_QUERY: u8 = 0;
const ICP_PRECOMPILE_KIND_UPDATE: u8 = 1;
const COMPACT_PRINCIPAL_FIELD_LEN: usize = 1 + MAX_PRINCIPAL_LEN;
const COMPACT_UNWRAP_INPUT_LEN: usize = 1 + COMPACT_PRINCIPAL_FIELD_LEN * 2 + 32;
const COMPACT_NATIVE_WITHDRAW_INPUT_LEN: usize = 1 + COMPACT_PRINCIPAL_FIELD_LEN;
const ABI_DYNAMIC_FIELDS: usize = 2;
const NATIVE_WITHDRAW_FIELDS: usize = 2;
const WRAP_FACTORY_STORAGE_TOKEN_BY_ASSET_KEY_SLOT: u64 = 0;
const WRAPPED_TOKEN_TOTAL_SUPPLY_SLOT: u64 = 2;
const WRAPPED_TOKEN_BALANCE_OF_SLOT: u64 = 3;
const WRAPPED_TOKEN_ALLOWANCE_SLOT: u64 = 4;
const UNWRAP_BURN_GAS_SURCHARGE: u64 = 45_000;
pub const WEI_PER_E8S: u128 = 10_000_000_000;
const FIXED_PRECOMPILE_GAS_RATIO_NUMERATOR: u32 = 1;
const FIXED_PRECOMPILE_GAS_RATIO_DENOMINATOR: u32 = 100;
const ICP_QUERY_BASE_GAS: u64 = 50_000;
const ICP_QUERY_INPUT_BYTE_GAS: u64 = 16;
const ICP_QUERY_REPLY_BYTE_GAS: u64 = 8;
const ICP_UPDATE_BASE_GAS: u64 = 80_000;
const ICP_UPDATE_INPUT_BYTE_GAS: u64 = 16;
const ICP_UPDATE_LOG_BYTE_GAS: u64 = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrecompileProfileEntry {
    pub address: [u8; 20],
    pub calls: u64,
    pub total_instructions: u128,
    pub avg_instructions: u64,
    pub max_instructions: u64,
    pub total_extra_gas: u128,
    pub avg_extra_gas: u64,
    pub max_extra_gas: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct PrecompileProfileAccumulator {
    calls: u64,
    total_instructions: u128,
    max_instructions: u64,
    total_extra_gas: u128,
    max_extra_gas: u64,
}

thread_local! {
    static PRECOMPILE_PROFILE_ACC: RefCell<BTreeMap<[u8; 20], PrecompileProfileAccumulator>> = const { RefCell::new(BTreeMap::new()) };
    static ICP_QUERY_CONTEXT: RefCell<IcpQueryExecutionContext> = RefCell::new(IcpQueryExecutionContext::disabled());
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static PRECOMPILE_INSTRUCTION_COUNTER_FOR_TEST: Cell<u64> = const { Cell::new(0) };
    static PRECOMPILE_INSTRUCTION_COUNTER_STEP_FOR_TEST: Cell<u64> = const { Cell::new(0) };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnwrapIntent {
    pub asset_id: Vec<u8>,
    pub amount: [u8; 32],
    pub recipient: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeWithdrawIntent {
    pub amount_e8s: [u8; 32],
    pub recipient: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IcpQueryRequest {
    pub target: Vec<u8>,
    pub method: String,
    pub arg: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IcpUpdateIntent {
    pub target: Vec<u8>,
    pub method: String,
    pub arg: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IcpQueryReply {
    Ok(Vec<u8>),
    Err(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum IcpQueryMode {
    Disabled,
    Detect,
    Reply {
        expected: IcpQueryRequest,
        reply: IcpQueryReply,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IcpQueryExecutionContext {
    mode: IcpQueryMode,
    calls: u8,
    pending: Option<IcpQueryRequest>,
}

impl IcpQueryExecutionContext {
    fn disabled() -> Self {
        Self {
            mode: IcpQueryMode::Disabled,
            calls: 0,
            pending: None,
        }
    }
}

struct IcpQueryContextGuard {
    prior: Option<IcpQueryExecutionContext>,
}

impl IcpQueryContextGuard {
    fn enter_detection() -> Self {
        let prior = ICP_QUERY_CONTEXT.with(|cell| {
            let mut ctx = cell.borrow_mut();
            let prior = ctx.clone();
            if matches!(ctx.mode, IcpQueryMode::Disabled) {
                *ctx = IcpQueryExecutionContext {
                    mode: IcpQueryMode::Detect,
                    calls: 0,
                    pending: None,
                };
            }
            prior
        });
        Self { prior: Some(prior) }
    }

    fn enter_reply(expected: IcpQueryRequest, reply: IcpQueryReply) -> Self {
        let prior = ICP_QUERY_CONTEXT.with(|cell| {
            let mut ctx = cell.borrow_mut();
            let prior = ctx.clone();
            *ctx = IcpQueryExecutionContext {
                mode: IcpQueryMode::Reply { expected, reply },
                calls: 0,
                pending: None,
            };
            prior
        });
        Self { prior: Some(prior) }
    }
}

impl Drop for IcpQueryContextGuard {
    fn drop(&mut self) {
        let Some(prior) = self.prior.take() else {
            return;
        };
        ICP_QUERY_CONTEXT.with(|cell| {
            *cell.borrow_mut() = prior;
        });
    }
}

#[derive(Clone, Debug)]
pub struct KasanePrecompileProvider {
    inner: EthPrecompiles,
    access: PrecompileAccess,
    update_allowlist: BTreeSet<Vec<u8>>,
    icp_update_intent_reserved: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrecompileAccess {
    pub wrap_side_effects: bool,
    pub icp_query: bool,
    pub icp_update_intent: bool,
    pub icp_update_intent_reserved: Option<usize>,
}

impl PrecompileAccess {
    pub const fn disabled() -> Self {
        Self {
            wrap_side_effects: false,
            icp_query: false,
            icp_update_intent: false,
            icp_update_intent_reserved: None,
        }
    }

    pub const fn wrap_side_effects() -> Self {
        Self {
            wrap_side_effects: true,
            icp_query: false,
            icp_update_intent: true,
            icp_update_intent_reserved: None,
        }
    }

    pub const fn wrap_side_effects_with_icp_update_reserved(reserved: usize) -> Self {
        Self {
            wrap_side_effects: true,
            icp_query: false,
            icp_update_intent: true,
            icp_update_intent_reserved: Some(reserved),
        }
    }

    pub const fn icp_query() -> Self {
        Self {
            wrap_side_effects: false,
            icp_query: true,
            icp_update_intent: false,
            icp_update_intent_reserved: None,
        }
    }
}

impl KasanePrecompileProvider {
    pub fn new(access: PrecompileAccess) -> Self {
        Self::with_update_allowlist(access, BTreeSet::new())
    }

    pub fn with_update_allowlist(
        access: PrecompileAccess,
        update_allowlist: BTreeSet<Vec<u8>>,
    ) -> Self {
        Self {
            inner: EthPrecompiles::default(),
            access,
            update_allowlist,
            icp_update_intent_reserved: access.icp_update_intent_reserved,
        }
    }
}

impl<CTX> PrecompileProvider<CTX> for KasanePrecompileProvider
where
    CTX: ContextTr<Cfg: Cfg>,
{
    type Output = InterpreterResult;

    fn set_spec(&mut self, spec: <CTX::Cfg as Cfg>::Spec) -> bool {
        <EthPrecompiles as PrecompileProvider<CTX>>::set_spec(&mut self.inner, spec)
    }

    fn run(
        &mut self,
        context: &mut CTX,
        inputs: &CallInputs,
    ) -> Result<Option<Self::Output>, String> {
        // 課金と profile 判断は IC instruction counter を正とする。
        let started_instruction = current_instruction_counter();
        let address = inputs.bytecode_address.into_array();

        let output = match inputs.bytecode_address {
            WRAP_PRECOMPILE_ADDRESS => Some(run_wrap_precompile(
                context,
                inputs,
                self.access.wrap_side_effects,
            )),
            NATIVE_WITHDRAW_PRECOMPILE_ADDRESS => Some(run_native_withdraw_precompile(
                context,
                inputs,
                self.access.wrap_side_effects,
            )),
            ICP_QUERY_PRECOMPILE_ADDRESS => Some(run_icp_query_precompile(
                context,
                inputs,
                self.access.icp_query,
            )),
            ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS => {
                let remaining_capacity = self.icp_update_intent_reserved.map(|reserved| {
                    let existing = evm_db::stable_state::with_state(|state| {
                        usize::try_from(*state.icp_update_active_count.get()).unwrap_or(usize::MAX)
                    });
                    let journaled = context
                        .journal()
                        .logs()
                        .iter()
                        .filter(|log| is_icp_update_intent_revm_log(log))
                        .count();
                    MAX_ICP_UPDATE_REQUESTS
                        .saturating_sub(existing.saturating_add(reserved).saturating_add(journaled))
                });
                Some(run_icp_update_intent_precompile(
                    context,
                    inputs,
                    self.access.icp_update_intent,
                    &self.update_allowlist,
                    remaining_capacity,
                ))
            }
            _ => self.inner.run(context, inputs)?,
        };

        let Some(mut out) = output else {
            return Ok(None);
        };

        let elapsed_instruction = current_instruction_counter().saturating_sub(started_instruction);
        let extra_gas = extra_gas_for_precompile(address, elapsed_instruction);
        if extra_gas != 0 && !out.gas.record_cost(extra_gas) {
            out = InterpreterResult {
                result: InstructionResult::PrecompileOOG,
                gas: Gas::new(inputs.gas_limit),
                output: Bytes::new(),
            };
        }
        record_precompile_profile(address, elapsed_instruction, extra_gas);
        Ok(Some(out))
    }

    fn warm_addresses(&self) -> Box<impl Iterator<Item = Address>> {
        let mut addresses = vec![
            WRAP_PRECOMPILE_ADDRESS,
            NATIVE_WITHDRAW_PRECOMPILE_ADDRESS,
            ICP_QUERY_PRECOMPILE_ADDRESS,
            ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS,
        ];
        addresses.extend(self.inner.warm_addresses());
        Box::new(addresses.into_iter())
    }

    fn contains(&self, address: &Address) -> bool {
        *address == WRAP_PRECOMPILE_ADDRESS
            || *address == NATIVE_WITHDRAW_PRECOMPILE_ADDRESS
            || *address == ICP_QUERY_PRECOMPILE_ADDRESS
            || *address == ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS
            || self.inner.contains(address)
    }
}

pub fn with_icp_query_detection<T>(f: impl FnOnce() -> T) -> (T, Option<IcpQueryRequest>) {
    let guard = IcpQueryContextGuard::enter_detection();
    let out = f();
    let pending = ICP_QUERY_CONTEXT.with(|cell| {
        let mut ctx = cell.borrow_mut();
        ctx.pending.take()
    });
    drop(guard);
    (out, pending)
}

pub fn with_icp_query_reply<T>(
    expected: IcpQueryRequest,
    reply: IcpQueryReply,
    f: impl FnOnce() -> T,
) -> T {
    let guard = IcpQueryContextGuard::enter_reply(expected, reply);
    let out = f();
    drop(guard);
    out
}

fn run_wrap_precompile<CTX: ContextTr>(
    context: &mut CTX,
    inputs: &CallInputs,
    allow_external: bool,
) -> InterpreterResult {
    let gas_limit = inputs.gas_limit;

    if !allow_external {
        return precompile_fail(context, gas_limit, "wrap.precompile.query_disallowed");
    }
    if inputs.is_static {
        return precompile_fail(context, gas_limit, "wrap.precompile.static_disallowed");
    }

    let input = inputs.input.bytes(context);
    let parsed = match parse_input(&input) {
        Ok(v) => v,
        Err(code) => return precompile_fail(context, gas_limit, code),
    };
    if let Err(code) = burn_wrapped_asset(context, unwrap_owner(inputs), &parsed) {
        return precompile_fail(context, gas_limit, &code);
    }
    let log_data = encode_log_data(&parsed);
    let log_data_len = log_data.len();
    let log = Log::new_unchecked(
        WRAP_PRECOMPILE_ADDRESS,
        vec![B256::from(wrap_event_topic0())],
        log_data.into(),
    );
    context.journal_mut().log(log);

    let mut out = InterpreterResult {
        result: InstructionResult::Return,
        gas: Gas::new(gas_limit),
        output: Bytes::new(),
    };
    let estimated_gas = estimate_wrap_precompile_gas(input.len(), log_data_len, ABI_DYNAMIC_FIELDS);
    if !out.gas.record_cost(estimated_gas) {
        return InterpreterResult {
            result: InstructionResult::PrecompileOOG,
            gas: Gas::new(gas_limit),
            output: Bytes::new(),
        };
    }
    out
}

fn run_native_withdraw_precompile<CTX: ContextTr>(
    context: &mut CTX,
    inputs: &CallInputs,
    allow_external: bool,
) -> InterpreterResult {
    let gas_limit = inputs.gas_limit;

    if !allow_external {
        return precompile_fail(
            context,
            gas_limit,
            "native_withdraw.precompile.query_disallowed",
        );
    }
    if inputs.is_static {
        return precompile_fail(
            context,
            gas_limit,
            "native_withdraw.precompile.static_disallowed",
        );
    }

    let input = inputs.input.bytes(context);
    let recipient = match parse_native_withdraw_input(&input) {
        Ok(v) => v,
        Err(code) => return precompile_fail(context, gas_limit, code),
    };
    let value = inputs.call_value();
    let Some(amount_e8s) = native_value_to_e8s(value) else {
        return precompile_fail(context, gas_limit, "native_withdraw.amount_not_e8s_aligned");
    };
    if amount_e8s.is_zero() {
        return precompile_fail(context, gas_limit, "native_withdraw.amount_zero");
    }
    let parsed = NativeWithdrawIntent {
        amount_e8s: amount_e8s.to_be_bytes(),
        recipient,
    };
    let log_data = encode_native_withdraw_log_data(&parsed);
    let log_data_len = log_data.len();
    let log = Log::new_unchecked(
        NATIVE_WITHDRAW_PRECOMPILE_ADDRESS,
        vec![B256::from(native_withdraw_event_topic0())],
        log_data.into(),
    );
    context.journal_mut().log(log);

    let mut out = InterpreterResult {
        result: InstructionResult::Return,
        gas: Gas::new(gas_limit),
        output: Bytes::new(),
    };
    let estimated_gas =
        estimate_wrap_precompile_gas(input.len(), log_data_len, NATIVE_WITHDRAW_FIELDS);
    if !out.gas.record_cost(estimated_gas) {
        return InterpreterResult {
            result: InstructionResult::PrecompileOOG,
            gas: Gas::new(gas_limit),
            output: Bytes::new(),
        };
    }
    out
}

fn run_icp_query_precompile<CTX: ContextTr>(
    context: &mut CTX,
    inputs: &CallInputs,
    allow_external: bool,
) -> InterpreterResult {
    let gas_limit = inputs.gas_limit;
    if !allow_external {
        return precompile_fail(context, gas_limit, "ic_query.precompile.query_disallowed");
    }
    if !inputs.call_value().is_zero() {
        return precompile_fail(context, gas_limit, "ic_query.value_disallowed");
    }

    let input = inputs.input.bytes(context);
    let request = match parse_icp_query_input(&input) {
        Ok(value) => value,
        Err(code) => return precompile_fail(context, gas_limit, code),
    };
    let reply = match resolve_icp_query_reply(request) {
        Ok(value) => value,
        Err("ic_query.pending") => return precompile_fail(context, gas_limit, "ic_query.pending"),
        Err(code) => return precompile_fail(context, gas_limit, code),
    };

    match reply {
        IcpQueryReply::Ok(bytes) => icp_query_return(input.len(), bytes, gas_limit),
        IcpQueryReply::Err(code) => precompile_fail(context, gas_limit, &code),
    }
}

fn resolve_icp_query_reply(request: IcpQueryRequest) -> Result<IcpQueryReply, &'static str> {
    ICP_QUERY_CONTEXT.with(|cell| {
        let mut ctx = cell.borrow_mut();
        if ctx.calls != 0 {
            return Err("ic_query.call_limit");
        }
        ctx.calls = ctx.calls.saturating_add(1);
        match &mut ctx.mode {
            IcpQueryMode::Disabled => Err("ic_query.async_context_required"),
            IcpQueryMode::Detect => {
                ctx.pending = Some(request);
                Err("ic_query.pending")
            }
            IcpQueryMode::Reply { expected, reply } => {
                if request != *expected {
                    return Err("ic_query.request_mismatch");
                }
                Ok(reply.clone())
            }
        }
    })
}

fn icp_query_return(input_len: usize, output: Vec<u8>, gas_limit: u64) -> InterpreterResult {
    let mut out = InterpreterResult {
        result: InstructionResult::Return,
        gas: Gas::new(gas_limit),
        output: output.into(),
    };
    let estimated_gas = ICP_QUERY_BASE_GAS
        .saturating_add(ICP_QUERY_INPUT_BYTE_GAS.saturating_mul(input_len as u64))
        .saturating_add(ICP_QUERY_REPLY_BYTE_GAS.saturating_mul(out.output.len() as u64));
    if !out.gas.record_cost(estimated_gas) {
        return InterpreterResult {
            result: InstructionResult::PrecompileOOG,
            gas: Gas::new(gas_limit),
            output: Bytes::new(),
        };
    }
    out
}

fn run_icp_update_intent_precompile<CTX: ContextTr>(
    context: &mut CTX,
    inputs: &CallInputs,
    allow_external: bool,
    update_allowlist: &BTreeSet<Vec<u8>>,
    remaining_capacity: Option<usize>,
) -> InterpreterResult {
    let gas_limit = inputs.gas_limit;
    if !allow_external {
        return precompile_fail(
            context,
            gas_limit,
            "ic_update.precompile.external_disallowed",
        );
    }
    if inputs.is_static {
        return precompile_fail(context, gas_limit, "ic_update.static_disallowed");
    }
    if !inputs.call_value().is_zero() {
        return precompile_fail(context, gas_limit, "ic_update.value_disallowed");
    }

    let input = inputs.input.bytes(context);
    let intent = match parse_icp_update_intent_input(&input) {
        Ok(value) => value,
        Err(code) => return precompile_fail(context, gas_limit, code),
    };
    if !update_allowlist.contains(&precompile_allow_key(&intent.target, &intent.method)) {
        return precompile_fail(context, gas_limit, "ic_update.allowlist_miss");
    }
    if matches!(remaining_capacity, Some(0)) {
        return precompile_fail(context, gas_limit, "ic_update.capacity_exceeded");
    }
    let log_data = encode_icp_update_intent_log_data(&intent);
    let log_data_len = log_data.len();
    let estimated_gas = ICP_UPDATE_BASE_GAS
        .saturating_add(ICP_UPDATE_INPUT_BYTE_GAS.saturating_mul(input.len() as u64))
        .saturating_add(ICP_UPDATE_LOG_BYTE_GAS.saturating_mul(log_data_len as u64));
    let mut out = InterpreterResult {
        result: InstructionResult::Return,
        gas: Gas::new(gas_limit),
        output: Bytes::new(),
    };
    if !out.gas.record_cost(estimated_gas) {
        return InterpreterResult {
            result: InstructionResult::PrecompileOOG,
            gas: Gas::new(gas_limit),
            output: Bytes::new(),
        };
    }
    let log = Log::new_unchecked(
        ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS,
        vec![B256::from(icp_update_intent_event_topic0())],
        log_data.into(),
    );
    context.journal_mut().log(log);
    out
}

fn is_icp_update_intent_revm_log(log: &Log) -> bool {
    log.address == ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS
        && log.data.topics().len() == 1
        && log.data.topics()[0] == B256::from(icp_update_intent_event_topic0())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn configure_precompile_instruction_counter_for_test(start: u64, step: u64) {
    PRECOMPILE_INSTRUCTION_COUNTER_FOR_TEST.with(|counter| counter.set(start));
    PRECOMPILE_INSTRUCTION_COUNTER_STEP_FOR_TEST.with(|counter| counter.set(step));
}

pub fn precompile_allow_key(target: &[u8], method: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + target.len() + method.len());
    out.push(target.len() as u8);
    out.extend_from_slice(target);
    out.extend_from_slice(method.as_bytes());
    out
}

fn precompile_fail<CTX: ContextTr>(
    context: &mut CTX,
    gas_limit: u64,
    msg: &str,
) -> InterpreterResult {
    context
        .local_mut()
        .set_precompile_error_context(msg.to_string());
    InterpreterResult {
        result: InstructionResult::PrecompileError,
        gas: Gas::new(gas_limit),
        output: Bytes::new(),
    }
}

fn parse_input(input: &[u8]) -> Result<UnwrapIntent, &'static str> {
    parse_compact_input(input)
}

fn parse_compact_input(input: &[u8]) -> Result<UnwrapIntent, &'static str> {
    if input.len() != COMPACT_UNWRAP_INPUT_LEN {
        return Err("wrap.arg.abi_invalid");
    }
    if input[0] != COMPACT_UNWRAP_FORMAT_VERSION {
        return Err("wrap.arg.abi_invalid");
    }
    let mut offset = 1usize;
    let asset_id = read_compact_principal(input, &mut offset)?;
    let amount = read_array_32(input, &mut offset).ok_or("wrap.arg.amount_invalid")?;
    let recipient = read_compact_principal(input, &mut offset)?;
    if offset != input.len() {
        return Err("wrap.arg.abi_invalid");
    }
    Ok(UnwrapIntent {
        asset_id,
        amount,
        recipient,
    })
}

fn parse_native_withdraw_input(input: &[u8]) -> Result<Vec<u8>, &'static str> {
    if input.len() != COMPACT_NATIVE_WITHDRAW_INPUT_LEN {
        return Err("native_withdraw.arg.abi_invalid");
    }
    if input[0] != COMPACT_NATIVE_WITHDRAW_FORMAT_VERSION {
        return Err("native_withdraw.arg.abi_invalid");
    }
    let mut offset = 1usize;
    let recipient = read_compact_principal(input, &mut offset)?;
    if recipient == [4u8] {
        return Err("native_withdraw.recipient_anonymous");
    }
    if offset != input.len() {
        return Err("native_withdraw.arg.abi_invalid");
    }
    Ok(recipient)
}

fn parse_icp_query_input(input: &[u8]) -> Result<IcpQueryRequest, &'static str> {
    if input.len() < 8 {
        return Err("ic_query.arg.abi_invalid");
    }
    let mut offset = 0usize;
    let version = read_u8(input, &mut offset).ok_or("ic_query.arg.abi_invalid")?;
    if version != COMPACT_ICP_PRECOMPILE_FORMAT_VERSION {
        return Err("ic_query.arg.version_invalid");
    }
    let kind = read_u8(input, &mut offset).ok_or("ic_query.arg.abi_invalid")?;
    if kind == ICP_PRECOMPILE_KIND_UPDATE {
        return Err("ic_query.update_unimplemented");
    }
    if kind != ICP_QUERY_KIND_QUERY {
        return Err("ic_query.kind_invalid");
    }
    let target = read_query_principal(input, &mut offset)?;
    let method_len = read_u8(input, &mut offset).ok_or("ic_query.arg.abi_invalid")? as usize;
    if method_len == 0 || method_len > MAX_QUERY_METHOD_LEN {
        return Err("ic_query.method_invalid");
    }
    let method_bytes =
        read_exact(input, &mut offset, method_len).ok_or("ic_query.method_invalid")?;
    let method = std::str::from_utf8(method_bytes)
        .map_err(|_| "ic_query.method_invalid")?
        .to_string();
    let arg_len = read_u32_be(input, &mut offset).ok_or("ic_query.arg.abi_invalid")? as usize;
    if arg_len > MAX_ICP_QUERY_ARG_LEN {
        return Err("ic_query.arg.too_large");
    }
    let arg = read_exact(input, &mut offset, arg_len)
        .ok_or("ic_query.arg.abi_invalid")?
        .to_vec();
    if offset != input.len() {
        return Err("ic_query.arg.abi_invalid");
    }
    Ok(IcpQueryRequest {
        target,
        method,
        arg,
    })
}

fn parse_icp_update_intent_input(input: &[u8]) -> Result<IcpUpdateIntent, &'static str> {
    if input.len() < 8 {
        return Err("ic_update.arg.abi_invalid");
    }
    let mut offset = 0usize;
    let version = read_u8(input, &mut offset).ok_or("ic_update.arg.abi_invalid")?;
    if version != COMPACT_ICP_PRECOMPILE_FORMAT_VERSION {
        return Err("ic_update.arg.version_invalid");
    }
    let kind = read_u8(input, &mut offset).ok_or("ic_update.arg.abi_invalid")?;
    if kind != ICP_PRECOMPILE_KIND_UPDATE {
        return Err("ic_update.kind_invalid");
    }
    let target = read_ic_update_principal(input, &mut offset)?;
    let method_len = read_u8(input, &mut offset).ok_or("ic_update.arg.abi_invalid")? as usize;
    if method_len == 0 || method_len > MAX_QUERY_METHOD_LEN {
        return Err("ic_update.method_invalid");
    }
    let method_bytes =
        read_exact(input, &mut offset, method_len).ok_or("ic_update.method_invalid")?;
    if !method_bytes.is_ascii() {
        return Err("ic_update.method_invalid");
    }
    let method = std::str::from_utf8(method_bytes)
        .map_err(|_| "ic_update.method_invalid")?
        .to_string();
    let arg_len = read_u32_be(input, &mut offset).ok_or("ic_update.arg.abi_invalid")? as usize;
    if arg_len > MAX_ICP_UPDATE_ARG_LEN {
        return Err("ic_update.arg.too_large");
    }
    let arg = read_exact(input, &mut offset, arg_len)
        .ok_or("ic_update.arg.abi_invalid")?
        .to_vec();
    if offset != input.len() {
        return Err("ic_update.arg.abi_invalid");
    }
    Ok(IcpUpdateIntent {
        target,
        method,
        arg,
    })
}

fn native_value_to_e8s(value: U256) -> Option<U256> {
    let unit = U256::from(WEI_PER_E8S);
    let rem = value % unit;
    if !rem.is_zero() {
        return None;
    }
    Some(value / unit)
}

pub(crate) fn estimate_wrap_precompile_gas(
    input_len: usize,
    log_data_len: usize,
    field_count: usize,
) -> u64 {
    let base_gas = 25_000u64.saturating_add(UNWRAP_BURN_GAS_SURCHARGE);
    let per_byte_gas = 16u64.saturating_mul(input_len as u64);
    let per_field_gas = 200u64.saturating_mul(field_count as u64);
    let topic_count = 1u64;
    let log_gas = 375u64
        .saturating_add(375u64.saturating_mul(topic_count))
        .saturating_add(8u64.saturating_mul(log_data_len as u64));
    base_gas
        .saturating_add(per_byte_gas)
        .saturating_add(per_field_gas)
        .saturating_add(log_gas)
}

// 前提:
// - unwrap は新 factory 配下 token のみを正とする
// - storage layout は tools/wrapper-vite/contracts 配下の現行 audited 実装に合わせる
// - burn は precompile 内で完結させ、成功時のみ unwrap intent log を積む
fn unwrap_owner(inputs: &CallInputs) -> Address {
    // unwrap の owner は tx origin ではなく、この precompile を呼んだ call frame の sender。
    inputs.caller
}

fn burn_wrapped_asset<CTX: ContextTr>(
    context: &mut CTX,
    owner: Address,
    intent: &UnwrapIntent,
) -> Result<(), String> {
    let factory = current_wrap_factory_address();
    let amount = U256::from_be_bytes(intent.amount);
    let asset_key = compute_asset_key(intent.asset_id.as_slice());
    let token_address = load_factory_token_address(context, factory, asset_key)?;
    if token_address == Address::ZERO {
        return Err("unwrap.token_not_deployed".to_string());
    }

    let balance_slot = address_mapping_slot(owner, WRAPPED_TOKEN_BALANCE_OF_SLOT);
    let allowance_slot = allowance_slot(owner, factory);
    let total_supply_slot = U256::from(WRAPPED_TOKEN_TOTAL_SUPPLY_SLOT);
    let mut approval_log_value = None;
    {
        let mut token_account = context
            .journal_mut()
            .load_account_mut(token_address)
            .map_err(|err| format!("wrap.burn.account_load_failed:{err:?}"))?;
        let token = &mut token_account.data;
        let balance = token
            .sload(balance_slot, false)
            .map_err(|err| format!("wrap.burn.storage_read_failed:{err:?}"))?
            .data
            .present_value();
        if balance < amount {
            return Err("erc20.insufficient_balance".to_string());
        }

        let allowance = token
            .sload(allowance_slot, false)
            .map_err(|err| format!("wrap.burn.storage_read_failed:{err:?}"))?
            .data
            .present_value();
        if allowance != U256::MAX {
            if allowance < amount {
                return Err("erc20.insufficient_allowance".to_string());
            }
            let next_allowance = allowance - amount;
            token
                .sstore(allowance_slot, next_allowance, false)
                .map_err(|err| format!("wrap.burn.storage_write_failed:{err:?}"))?;
            approval_log_value = Some(next_allowance);
        }

        let total_supply = token
            .sload(total_supply_slot, false)
            .map_err(|err| format!("wrap.burn.storage_read_failed:{err:?}"))?
            .data
            .present_value();
        if total_supply < amount {
            return Err("erc20.insufficient_balance".to_string());
        }
        let next_balance = balance - amount;
        let next_total_supply = total_supply - amount;
        token
            .sstore(balance_slot, next_balance, false)
            .map_err(|err| format!("wrap.burn.storage_write_failed:{err:?}"))?;
        token
            .sstore(total_supply_slot, next_total_supply, false)
            .map_err(|err| format!("wrap.burn.storage_write_failed:{err:?}"))?;
    }
    if let Some(next_allowance) = approval_log_value {
        emit_approval_log(context, token_address, owner, factory, next_allowance);
    }
    emit_transfer_log(context, token_address, owner, Address::ZERO, amount);
    Ok(())
}

fn current_wrap_factory_address() -> Address {
    let raw = current_runtime_config()
        .wrap_factory_address()
        .unwrap_or_else(|err| ic_cdk::trap(format!("InvalidRuntimeConfig: {err}")));
    Address::new(raw)
}

fn compute_asset_key(asset_id: &[u8]) -> [u8; 32] {
    let mut payload = Vec::with_capacity(14 + 32 + asset_id.len());
    payload.extend_from_slice(b"kasane.wrap.v1");
    payload.extend_from_slice(&U256::from(CHAIN_ID).to_be_bytes::<32>());
    payload.extend_from_slice(asset_id);
    hash::keccak256(&payload)
}

fn load_factory_token_address<CTX: ContextTr>(
    context: &mut CTX,
    factory: Address,
    asset_key: [u8; 32],
) -> Result<Address, String> {
    let slot = mapping_slot(
        B256::from(asset_key),
        U256::from(WRAP_FACTORY_STORAGE_TOKEN_BY_ASSET_KEY_SLOT),
    );
    let mut factory_account = context
        .journal_mut()
        .load_account_mut(factory)
        .map_err(|err| format!("wrap.factory.account_load_failed:{err:?}"))?;
    let raw = factory_account
        .data
        .sload(slot, false)
        .map_err(|err| format!("wrap.factory.storage_read_failed:{err:?}"))?
        .data
        .present_value()
        .to_be_bytes::<32>();
    let mut address = [0u8; 20];
    address.copy_from_slice(&raw[12..]);
    Ok(Address::new(address))
}

fn mapping_slot(key: B256, slot: U256) -> U256 {
    let mut input = [0u8; 64];
    input[..32].copy_from_slice(key.as_slice());
    input[32..].copy_from_slice(&slot.to_be_bytes::<32>());
    U256::from_be_bytes(hash::keccak256(&input))
}

fn address_mapping_slot(key: Address, slot: u64) -> U256 {
    let mut key_bytes = [0u8; 32];
    key_bytes[12..].copy_from_slice(key.as_slice());
    mapping_slot(B256::from(key_bytes), U256::from(slot))
}

fn allowance_slot(owner: Address, spender: Address) -> U256 {
    let outer = address_mapping_slot(owner, WRAPPED_TOKEN_ALLOWANCE_SLOT);
    let mut spender_bytes = [0u8; 32];
    spender_bytes[12..].copy_from_slice(spender.as_slice());
    mapping_slot(B256::from(spender_bytes), outer)
}

fn emit_approval_log<CTX: ContextTr>(
    context: &mut CTX,
    token: Address,
    owner: Address,
    spender: Address,
    value: U256,
) {
    let log = Log::new_unchecked(
        token,
        vec![
            B256::from(approval_event_topic0()),
            topic_from_address(owner),
            topic_from_address(spender),
        ],
        value.to_be_bytes_vec().into(),
    );
    context.journal_mut().log(log);
}

fn emit_transfer_log<CTX: ContextTr>(
    context: &mut CTX,
    token: Address,
    from: Address,
    to: Address,
    value: U256,
) {
    let log = Log::new_unchecked(
        token,
        vec![
            B256::from(transfer_event_topic0()),
            topic_from_address(from),
            topic_from_address(to),
        ],
        value.to_be_bytes_vec().into(),
    );
    context.journal_mut().log(log);
}

fn topic_from_address(address: Address) -> B256 {
    let mut out = [0u8; 32];
    out[12..].copy_from_slice(address.as_slice());
    B256::from(out)
}

fn encode_log_data(intent: &UnwrapIntent) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + 32 + 2);
    out.push(intent.asset_id.len() as u8);
    out.extend_from_slice(&intent.asset_id);
    out.extend_from_slice(&intent.amount);
    out.push(intent.recipient.len() as u8);
    out.extend_from_slice(&intent.recipient);
    out
}

fn encode_native_withdraw_log_data(intent: &NativeWithdrawIntent) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + 1 + intent.recipient.len());
    out.extend_from_slice(&intent.amount_e8s);
    out.push(intent.recipient.len() as u8);
    out.extend_from_slice(&intent.recipient);
    out
}

fn encode_icp_update_intent_log_data(intent: &IcpUpdateIntent) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(6 + intent.target.len() + intent.method.len() + intent.arg.len());
    out.push(intent.target.len() as u8);
    out.extend_from_slice(&intent.target);
    out.push(intent.method.len() as u8);
    out.extend_from_slice(intent.method.as_bytes());
    out.extend_from_slice(&(intent.arg.len() as u32).to_be_bytes());
    out.extend_from_slice(&intent.arg);
    out
}

pub fn unwrap_intent_from_log(log: &LogEntry) -> Option<UnwrapIntent> {
    if log.address.into_array() != WRAP_PRECOMPILE_ADDRESS.into_array() {
        return None;
    }
    let topics = log.topics();
    if topics.len() != 1 || topics[0].0 != wrap_event_topic0() {
        return None;
    }
    let data = log.data.data.as_ref();
    let mut offset = 0usize;
    let asset_id = read_len_prefixed(data, &mut offset)?;
    let amount = read_array_32(data, &mut offset)?;
    let recipient = read_len_prefixed(data, &mut offset)?;
    if offset != data.len() {
        return None;
    }
    Some(UnwrapIntent {
        asset_id,
        amount,
        recipient,
    })
}

pub fn native_withdraw_intent_from_log(log: &LogEntry) -> Option<NativeWithdrawIntent> {
    if log.address.into_array() != NATIVE_WITHDRAW_PRECOMPILE_ADDRESS.into_array() {
        return None;
    }
    let topics = log.topics();
    if topics.len() != 1 || topics[0].0 != native_withdraw_event_topic0() {
        return None;
    }
    let data = log.data.data.as_ref();
    let mut offset = 0usize;
    let amount_e8s = read_array_32(data, &mut offset)?;
    let recipient = read_len_prefixed(data, &mut offset)?;
    if offset != data.len() {
        return None;
    }
    Some(NativeWithdrawIntent {
        amount_e8s,
        recipient,
    })
}

pub fn icp_update_intent_from_log(log: &LogEntry) -> Option<IcpUpdateIntent> {
    if log.address.into_array() != ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array() {
        return None;
    }
    let topics = log.topics();
    if topics.len() != 1 || topics[0].0 != icp_update_intent_event_topic0() {
        return None;
    }
    let data = log.data.data.as_ref();
    let mut offset = 0usize;
    let target = read_len_prefixed(data, &mut offset)?;
    if target.len() > MAX_PRINCIPAL_LEN {
        return None;
    }
    let method = read_len_prefixed(data, &mut offset)?;
    if method.len() > MAX_QUERY_METHOD_LEN || !method.is_ascii() {
        return None;
    }
    let arg_len = read_u32_be(data, &mut offset)? as usize;
    if arg_len > MAX_ICP_UPDATE_ARG_LEN {
        return None;
    }
    let arg = read_exact(data, &mut offset, arg_len)?.to_vec();
    if offset != data.len() {
        return None;
    }
    Some(IcpUpdateIntent {
        target,
        method: String::from_utf8(method).ok()?,
        arg,
    })
}

fn wrap_event_topic0() -> [u8; 32] {
    hash::keccak256(b"KasaneUnwrapRequest(bytes)")
}

fn native_withdraw_event_topic0() -> [u8; 32] {
    hash::keccak256(b"KasaneNativeWithdrawalRequest(bytes)")
}

fn icp_update_intent_event_topic0() -> [u8; 32] {
    hash::keccak256(b"KasaneIcpUpdateIntent(bytes)")
}

fn approval_event_topic0() -> [u8; 32] {
    hash::keccak256(b"Approval(address,address,uint256)")
}

fn transfer_event_topic0() -> [u8; 32] {
    hash::keccak256(b"Transfer(address,address,uint256)")
}

fn is_valid_principal_bytes(len: usize) -> bool {
    (1..=MAX_PRINCIPAL_LEN).contains(&len)
}

fn read_compact_principal(input: &[u8], offset: &mut usize) -> Result<Vec<u8>, &'static str> {
    let len = *input.get(*offset).ok_or("wrap.arg.abi_invalid")? as usize;
    *offset = offset.saturating_add(1);
    if !is_valid_principal_bytes(len) {
        return Err("wrap.arg.principal_invalid");
    }
    let end = offset
        .checked_add(MAX_PRINCIPAL_LEN)
        .ok_or("wrap.arg.abi_invalid")?;
    let slot = input.get(*offset..end).ok_or("wrap.arg.abi_invalid")?;
    if slot[len..].iter().any(|&byte| byte != 0) {
        return Err("wrap.arg.padding_invalid");
    }
    let bytes = slot[..len].to_vec();
    *offset = end;
    Ok(bytes)
}

fn read_query_principal(input: &[u8], offset: &mut usize) -> Result<Vec<u8>, &'static str> {
    let len = read_u8(input, offset).ok_or("ic_query.arg.abi_invalid")? as usize;
    if !is_valid_principal_bytes(len) {
        return Err("ic_query.target_invalid");
    }
    read_exact(input, offset, len)
        .map(|bytes| bytes.to_vec())
        .ok_or("ic_query.arg.abi_invalid")
}

fn read_ic_update_principal(input: &[u8], offset: &mut usize) -> Result<Vec<u8>, &'static str> {
    let len = read_u8(input, offset).ok_or("ic_update.arg.abi_invalid")? as usize;
    if !is_valid_principal_bytes(len) {
        return Err("ic_update.target_invalid");
    }
    read_exact(input, offset, len)
        .map(|bytes| bytes.to_vec())
        .ok_or("ic_update.arg.abi_invalid")
}

fn read_u8(data: &[u8], offset: &mut usize) -> Option<u8> {
    let value = *data.get(*offset)?;
    *offset = offset.saturating_add(1);
    Some(value)
}

fn read_u32_be(data: &[u8], offset: &mut usize) -> Option<u32> {
    let bytes = read_exact(data, offset, 4)?;
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

fn read_exact<'a>(data: &'a [u8], offset: &mut usize, len: usize) -> Option<&'a [u8]> {
    let end = offset.checked_add(len)?;
    let bytes = data.get(*offset..end)?;
    *offset = end;
    Some(bytes)
}

fn read_len_prefixed(data: &[u8], offset: &mut usize) -> Option<Vec<u8>> {
    let len = *data.get(*offset)? as usize;
    *offset = offset.saturating_add(1);
    if len == 0 || len > MAX_FIELD_LEN {
        return None;
    }
    let end = offset.checked_add(len)?;
    let bytes = data.get(*offset..end)?.to_vec();
    *offset = end;
    Some(bytes)
}

fn read_array_32(data: &[u8], offset: &mut usize) -> Option<[u8; 32]> {
    let end = offset.checked_add(32)?;
    let slice = data.get(*offset..end)?;
    let mut out = [0u8; 32];
    out.copy_from_slice(slice);
    *offset = end;
    Some(out)
}

fn current_instruction_counter() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        return ic_cdk::api::performance_counter(
            ic_cdk::api::PerformanceCounterType::InstructionCounter,
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        PRECOMPILE_INSTRUCTION_COUNTER_FOR_TEST.with(|counter| {
            let value = counter.get();
            let step = PRECOMPILE_INSTRUCTION_COUNTER_STEP_FOR_TEST.with(|step| step.get());
            counter.set(value.saturating_add(step));
            value
        })
    }
}

fn extra_gas_by_instruction_ratio(elapsed_instruction: u64) -> u64 {
    compute_extra_gas(
        elapsed_instruction,
        FIXED_PRECOMPILE_GAS_RATIO_NUMERATOR,
        FIXED_PRECOMPILE_GAS_RATIO_DENOMINATOR,
    )
}

fn extra_gas_for_precompile(address: [u8; 20], elapsed_instruction: u64) -> u64 {
    if address == WRAP_PRECOMPILE_ADDRESS.into_array() {
        return 0;
    }
    extra_gas_by_instruction_ratio(elapsed_instruction)
}

fn compute_extra_gas(elapsed_instruction: u64, numerator: u32, denominator: u32) -> u64 {
    if elapsed_instruction == 0 || numerator == 0 {
        return 0;
    }
    let denominator = denominator.max(1);
    let scaled = u128::from(elapsed_instruction).saturating_mul(u128::from(numerator));
    let rounded =
        scaled.saturating_add(u128::from(denominator).saturating_sub(1)) / u128::from(denominator);
    rounded.min(u128::from(u64::MAX)) as u64
}

fn record_precompile_profile(address: [u8; 20], elapsed_instruction: u64, extra_gas: u64) {
    PRECOMPILE_PROFILE_ACC.with(|map| {
        let mut map = map.borrow_mut();
        let entry = map
            .entry(address)
            .or_insert_with(PrecompileProfileAccumulator::default);
        entry.calls = entry.calls.saturating_add(1);
        entry.total_instructions = entry
            .total_instructions
            .saturating_add(u128::from(elapsed_instruction));
        entry.max_instructions = entry.max_instructions.max(elapsed_instruction);
        entry.total_extra_gas = entry.total_extra_gas.saturating_add(u128::from(extra_gas));
        entry.max_extra_gas = entry.max_extra_gas.max(extra_gas);
    });
}

pub fn precompile_profile_snapshot() -> Vec<PrecompileProfileEntry> {
    PRECOMPILE_PROFILE_ACC.with(|map| {
        map.borrow()
            .iter()
            .map(|(address, acc)| {
                let calls = acc.calls.max(1);
                PrecompileProfileEntry {
                    address: *address,
                    calls: acc.calls,
                    total_instructions: acc.total_instructions,
                    avg_instructions: (acc.total_instructions / u128::from(calls)) as u64,
                    max_instructions: acc.max_instructions,
                    total_extra_gas: acc.total_extra_gas,
                    avg_extra_gas: (acc.total_extra_gas / u128::from(calls)) as u64,
                    max_extra_gas: acc.max_extra_gas,
                }
            })
            .collect()
    })
}

pub fn clear_precompile_profile() {
    PRECOMPILE_PROFILE_ACC.with(|map| map.borrow_mut().clear());
}

#[cfg(test)]
#[path = "kasane_precompiles_tests.rs"]
mod tests;
