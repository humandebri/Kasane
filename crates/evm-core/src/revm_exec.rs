//! どこで: Phase1のREVM実行 / 何を: TxEnvの実行とcommit / なぜ: 状態更新をEVM経由にするため

use crate::bytes::try_address_to_bytes;
use crate::chain::{before_store_write_for_test, trap_store_err};
use crate::constants::FEE_RECIPIENT;
use crate::hash::keccak256;
use crate::revm_db::RevmStableDb;
use crate::tx_decode::DecodeError;
use crate::wrap_precompile::{
    with_icp_query_detection, with_icp_query_reply, IcpQueryReply, IcpQueryRequest,
    PrecompileAccess, WrapPrecompileProvider,
};
use evm_db::chain_data::constants::{
    CHAIN_ID, MAX_LOGS_PER_TX, MAX_LOG_DATA, MAX_LOG_TOPICS, MAX_RETURN_DATA,
};
use evm_db::chain_data::receipt::{log_entry_from_parts, LogEntry};
use evm_db::chain_data::{
    InternalTrace, InternalTraceActionKind, InternalTraceSet, ReceiptLike, TxId, TxIndexEntry,
    TxKind, MAX_INTERNAL_TRACES_PER_TX_U32,
};
use evm_db::stable_state::with_state_mut;
use evm_db::Storable;
use revm::context::{Context, TxEnv};
use revm::context_interface::result::{EVMError, ExecutionResult, HaltReason, InvalidTransaction};
use revm::context_interface::CreateScheme;
use revm::database_interface::{Database, DatabaseCommit};
use revm::handler::{ExecuteCommitEvm, MainBuilder, MainContext};
use revm::inspector::InspectEvm;
use revm::interpreter::{
    CallInputs, CallOutcome, CallScheme, CreateInputs, CreateOutcome, InstructionResult,
    Interpreter, InterpreterTypes,
};
use revm::primitives::{Address, U256};
use revm::state::Account;
#[cfg(not(target_arch = "wasm32"))]
use std::cell::Cell;

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static INSTRUCTION_COUNTER_FOR_TEST: Cell<u64> = const { Cell::new(0) };
    static INSTRUCTION_COUNTER_STEP_FOR_TEST: Cell<u64> = const { Cell::new(0) };
    static INSTRUCTION_BUDGET_TRIPPED_FOR_TEST: Cell<bool> = const { Cell::new(false) };
}

pub(crate) type StateDiff = revm::primitives::HashMap<Address, revm::state::Account>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecError {
    Decode(DecodeError),
    TxError(OpTransactionError),
    Revert,
    EvmHalt(OpHaltReason),
    ExecutionFailed,
    InvalidGasFee,
    ResultTooLarge,
    InstructionBudgetExceeded,
    SnapshotChanged,
    ExternalQuery(IcpQueryRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OpTransactionError {
    TxBuildFailed,
    TxRejectedByPolicy,
    TxPrecheckFailed,
    TxExecutionFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OpHaltReason {
    OutOfGas,
    InvalidOpcode,
    StackOverflow,
    StackUnderflow,
    InvalidJump,
    StateChangeDuringStaticCall,
    PrecompileError,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecPath {
    UserTx,
    SystemTx,
}

pub struct ExecOutcome {
    pub tx_id: TxId,
    pub tx_index: u32,
    pub receipt: ReceiptLike,
    pub return_data: Vec<u8>,
    pub final_status: String,
    pub halt_reason: Option<OpHaltReason>,
    pub internal_traces: InternalTraceSet,
}

#[derive(Clone, Debug)]
pub struct BlockExecContext {
    pub block_number: u64,
    pub timestamp: u64,
    pub base_fee: u64,
    pub block_gas_limit: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FeeBreakdown {
    l1_data_fee: u128,
    operator_fee: u128,
}

impl FeeBreakdown {
    fn total_fee(self, effective_gas_price: u64, gas_used: u64) -> u128 {
        verified_core::fee::total_fee(
            gas_used,
            effective_gas_price,
            self.l1_data_fee,
            self.operator_fee,
        )
    }
}

pub fn execute_tx(
    tx_id: TxId,
    tx_index: u32,
    _tx_kind: TxKind,
    _raw_tx: &[u8],
    tx_env: TxEnv,
    exec_ctx: &BlockExecContext,
    exec_path: ExecPath,
) -> Result<ExecOutcome, ExecError> {
    let (outcome, _state_diff) = execute_tx_on(
        RevmStableDb,
        tx_id,
        tx_index,
        tx_env,
        exec_ctx,
        exec_path,
        true,
        None,
        PrecompileAccess::wrap_side_effects(),
    )?;
    Ok(outcome)
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_tx_on_async<DB, R, Fut>(
    db: &mut DB,
    tx_id: TxId,
    tx_index: u32,
    tx_env: TxEnv,
    exec_ctx: &BlockExecContext,
    exec_path: ExecPath,
    persist_receipt_index: bool,
    instruction_soft_limit: Option<u64>,
    precompile_access: PrecompileAccess,
    mut resolver: R,
    validate_after_resolve: impl FnOnce() -> Result<(), ExecError>,
) -> Result<(ExecOutcome, StateDiff), ExecError>
where
    for<'a> &'a mut DB:
        revm::database_interface::Database<Error = core::convert::Infallible> + DatabaseCommit,
    R: FnMut(IcpQueryRequest) -> Fut,
    Fut: core::future::Future<Output = Result<Vec<u8>, String>>,
{
    let first = execute_tx_on(
        &mut *db,
        tx_id,
        tx_index,
        tx_env.clone(),
        exec_ctx,
        exec_path,
        persist_receipt_index,
        instruction_soft_limit,
        precompile_access,
    );
    let request = match first {
        Err(ExecError::ExternalQuery(request)) => request,
        other => return other,
    };
    let reply = match resolver(request.clone()).await {
        Ok(bytes) => IcpQueryReply::Ok(bytes),
        Err(code) => IcpQueryReply::Err(code),
    };
    validate_after_resolve()?;
    with_icp_query_reply(request, reply, || {
        execute_tx_on(
            &mut *db,
            tx_id,
            tx_index,
            tx_env,
            exec_ctx,
            exec_path,
            persist_receipt_index,
            instruction_soft_limit,
            precompile_access,
        )
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_tx_on<DB>(
    db: DB,
    tx_id: TxId,
    tx_index: u32,
    tx_env: TxEnv,
    exec_ctx: &BlockExecContext,
    exec_path: ExecPath,
    persist_receipt_index: bool,
    instruction_soft_limit: Option<u64>,
    precompile_access: PrecompileAccess,
) -> Result<(ExecOutcome, StateDiff), ExecError>
where
    DB: revm::database_interface::Database<Error = core::convert::Infallible> + DatabaseCommit,
{
    if exec_path == ExecPath::SystemTx {
        return Err(ExecError::TxError(OpTransactionError::TxRejectedByPolicy));
    }
    let effective_gas_price = compute_effective_gas_price(
        tx_env.gas_price,
        tx_env.gas_priority_fee.unwrap_or(0),
        exec_ctx.base_fee,
    )
    .ok_or(ExecError::InvalidGasFee)?;

    let inspector_limit = instruction_soft_limit.unwrap_or(0);
    let inspector = InspectorMux::new(inspector_limit, exec_ctx.block_number, tx_index);
    let mut evm = Context::mainnet()
        .with_db(db)
        .modify_cfg_chained(|cfg| {
            cfg.chain_id = CHAIN_ID;
        })
        .modify_block_chained(|block| {
            block.number = U256::from(exec_ctx.block_number);
            block.timestamp = U256::from(exec_ctx.timestamp);
            block.gas_limit = exec_ctx.block_gas_limit;
            block.basefee = exec_ctx.base_fee;
            block.beneficiary = FEE_RECIPIENT;
        })
        .build_mainnet_with_inspector(inspector)
        .with_precompiles(WrapPrecompileProvider::new(precompile_access));

    let (result, pending_query) =
        with_icp_query_detection(|| evm.inspect_tx(tx_env).map_err(map_tx_error_stage));
    if evm.inspector.budget.tripped {
        return Err(ExecError::InstructionBudgetExceeded);
    }
    if let Some(request) = pending_query {
        return Err(ExecError::ExternalQuery(request));
    }
    let result = result?;
    let (status, gas_used, output, contract_address, logs, final_status, halt_reason) =
        match result.result {
            ExecutionResult::Success {
                gas_used,
                output,
                logs,
                ..
            } => {
                let addr = output.address().map(|a| {
                    try_address_to_bytes(*a).expect("revm create output address must be 20 bytes")
                });
                let mapped = logs
                    .into_iter()
                    .map(revm_log_to_receipt_log)
                    .collect::<Vec<_>>();
                (
                    1u8,
                    gas_used,
                    output.data().as_ref().to_vec(),
                    addr,
                    mapped,
                    "Success".to_string(),
                    None,
                )
            }
            ExecutionResult::Revert { gas_used, output } => (
                0u8,
                gas_used,
                output.to_vec(),
                None,
                Vec::new(),
                "Revert".to_string(),
                None,
            ),
            ExecutionResult::Halt { gas_used, reason } => {
                let fixed = map_halt_reason(reason);
                (
                    0u8,
                    gas_used,
                    Vec::new(),
                    None,
                    Vec::new(),
                    format!("Halt:{fixed:?}"),
                    Some(fixed),
                )
            }
        };

    let mut state_diff = collect_state_diff(result.state);
    add_base_fee_portion_to_recipient(&mut state_diff, gas_used, exec_ctx.base_fee);
    commit_state_diff(&mut evm, state_diff.clone());

    validate_execution_result_sizes(&output, &logs)?;

    let fee_breakdown = FeeBreakdown {
        l1_data_fee: 0,
        operator_fee: 0,
    };

    let return_data_hash = keccak256(&output);
    let receipt = ReceiptLike {
        tx_id,
        block_number: exec_ctx.block_number,
        tx_index,
        status,
        gas_used,
        effective_gas_price,
        l1_data_fee: fee_breakdown.l1_data_fee,
        operator_fee: fee_breakdown.operator_fee,
        total_fee: fee_breakdown.total_fee(effective_gas_price, gas_used),
        return_data_hash,
        return_data: output.clone(),
        contract_address,
        logs,
    };

    if persist_receipt_index {
        store_receipt_index(tx_id, exec_ctx.block_number, tx_index, &receipt);
    }

    let outcome = ExecOutcome {
        tx_id,
        tx_index,
        receipt,
        return_data: output,
        final_status,
        halt_reason,
        internal_traces: evm.inspector.traces.finish(),
    };
    Ok((outcome, state_diff))
}

struct InspectorMux {
    budget: InstructionBudgetInspector,
    traces: InternalTraceInspector,
}

impl InspectorMux {
    fn new(limit: u64, block_number: u64, tx_index: u32) -> Self {
        Self {
            budget: InstructionBudgetInspector::new(limit),
            traces: InternalTraceInspector::new(block_number, tx_index),
        }
    }
}

struct InstructionBudgetInspector {
    start: u64,
    limit: u64,
    tripped: bool,
}

impl InstructionBudgetInspector {
    fn new(limit: u64) -> Self {
        Self {
            start: current_instruction_counter(),
            limit,
            tripped: instruction_budget_tripped_for_test(),
        }
    }
}

impl<CTX, INTR: InterpreterTypes> revm::Inspector<CTX, INTR> for InstructionBudgetInspector {
    fn step(&mut self, interp: &mut Interpreter<INTR>, _context: &mut CTX) {
        if self.limit == 0 || self.tripped {
            return;
        }
        let now = current_instruction_counter();
        if now.saturating_sub(self.start) >= self.limit {
            self.tripped = true;
            interp.halt(InstructionResult::OutOfGas);
        }
    }
}

#[derive(Clone, Debug)]
struct InternalTraceDraft {
    trace_id: Vec<u32>,
    trace: InternalTrace,
}

struct InternalTraceInspector {
    block_number: u64,
    tx_index: u32,
    frame_children: Vec<u32>,
    active_paths: Vec<Option<Vec<u32>>>,
    traces: Vec<InternalTraceDraft>,
    total_count: u32,
}

impl InternalTraceInspector {
    fn new(block_number: u64, tx_index: u32) -> Self {
        Self {
            block_number,
            tx_index,
            frame_children: vec![0],
            active_paths: Vec::new(),
            traces: Vec::new(),
            total_count: 0,
        }
    }

    fn finish(self) -> InternalTraceSet {
        InternalTraceSet::new_with_counts(
            self.traces.into_iter().map(|value| value.trace).collect(),
            self.total_count,
        )
    }

    fn start_call(&mut self, inputs: &CallInputs) {
        let action_kind = match inputs.scheme {
            CallScheme::Call => InternalTraceActionKind::Call,
            CallScheme::CallCode => InternalTraceActionKind::CallCode,
            CallScheme::DelegateCall => InternalTraceActionKind::DelegateCall,
            CallScheme::StaticCall => InternalTraceActionKind::StaticCall,
        };
        self.start_action(
            action_kind,
            inputs.caller.into_array(),
            Some(inputs.target_address.into_array()),
            None,
            inputs.call_value().to_be_bytes(),
        );
    }

    fn end_call(&mut self, outcome: &CallOutcome) {
        let status = trace_outcome_status(outcome.instruction_result());
        self.finish_action(status.0, status.1, None);
    }

    fn start_create(&mut self, inputs: &CreateInputs) {
        let action_kind = match inputs.scheme() {
            CreateScheme::Create => InternalTraceActionKind::Create,
            CreateScheme::Create2 { .. } => InternalTraceActionKind::Create2,
            // revm の Custom は CREATE2 ではなく、生成アドレスを外部指定する create。
            CreateScheme::Custom { .. } => InternalTraceActionKind::Custom,
        };
        self.start_action(
            action_kind,
            inputs.caller().into_array(),
            None,
            None,
            inputs.value().to_be_bytes(),
        );
    }

    fn end_create(&mut self, outcome: &CreateOutcome) {
        let status = trace_outcome_status(outcome.instruction_result());
        let created = outcome.address.map(Address::into_array);
        self.finish_action(status.0, status.1, created);
    }

    fn record_selfdestruct(&mut self, contract: Address, target: Address, value: U256) {
        if self.active_paths.is_empty() {
            return;
        }
        let Some(trace_id) = self.allocate_event_trace_id() else {
            return;
        };
        self.total_count = self.total_count.saturating_add(1);
        if self.traces.len() < MAX_INTERNAL_TRACES_PER_TX_U32 as usize {
            self.traces.push(InternalTraceDraft {
                trace_id: trace_id.clone(),
                trace: InternalTrace {
                    block_number: self.block_number,
                    tx_index: self.tx_index,
                    trace_id: render_trace_id(&trace_id),
                    depth: trace_id.len() as u16,
                    action_kind: InternalTraceActionKind::Selfdestruct,
                    from_address: contract.into_array(),
                    to_address: Some(target.into_array()),
                    value: value.to_be_bytes(),
                    created_contract_address: None,
                    success: true,
                    error_code: None,
                },
            });
        }
    }

    fn allocate_event_trace_id(&mut self) -> Option<Vec<u32>> {
        let next_child = self.frame_children.last_mut()?;
        let child_index = *next_child;
        *next_child = next_child.saturating_add(1);
        let mut trace_id = self
            .active_paths
            .last()
            .cloned()
            .flatten()
            .unwrap_or_default();
        trace_id.push(child_index);
        Some(trace_id)
    }

    fn start_action(
        &mut self,
        action_kind: InternalTraceActionKind,
        from_address: [u8; 20],
        to_address: Option<[u8; 20]>,
        created_contract_address: Option<[u8; 20]>,
        value: [u8; 32],
    ) {
        let trace_id = self.allocate_trace_id();
        let is_internal = !self.active_paths.is_empty();
        if is_internal {
            self.total_count = self.total_count.saturating_add(1);
            if self.traces.len() < MAX_INTERNAL_TRACES_PER_TX_U32 as usize {
                self.traces.push(InternalTraceDraft {
                    trace_id: trace_id.clone(),
                    trace: InternalTrace {
                        block_number: self.block_number,
                        tx_index: self.tx_index,
                        trace_id: render_trace_id(&trace_id),
                        depth: trace_id.len() as u16,
                        action_kind,
                        from_address,
                        to_address,
                        value,
                        created_contract_address,
                        success: false,
                        error_code: Some("pending".to_string()),
                    },
                });
            }
            self.active_paths.push(Some(trace_id));
            return;
        }
        self.active_paths.push(None);
    }

    fn finish_action(
        &mut self,
        success: bool,
        error_code: Option<String>,
        created_contract_address: Option<[u8; 20]>,
    ) {
        let active = self.active_paths.pop();
        let _ = self.frame_children.pop();
        let Some(Some(trace_id)) = active else {
            return;
        };
        if let Some(item) = self
            .traces
            .iter_mut()
            .rev()
            .find(|item| item.trace_id == trace_id)
        {
            item.trace.success = success;
            item.trace.error_code = error_code;
            if created_contract_address.is_some() {
                item.trace.created_contract_address = created_contract_address;
            }
        }
    }

    fn allocate_trace_id(&mut self) -> Vec<u32> {
        let next_child = self.frame_children.last_mut().expect("trace root frame");
        let child_index = *next_child;
        *next_child = next_child.saturating_add(1);
        let mut trace_id = self
            .active_paths
            .last()
            .cloned()
            .flatten()
            .unwrap_or_default();
        trace_id.push(child_index);
        self.frame_children.push(0);
        trace_id
    }
}

impl<CTX, INTR: InterpreterTypes> revm::Inspector<CTX, INTR> for InspectorMux {
    fn step(&mut self, interp: &mut Interpreter<INTR>, context: &mut CTX) {
        self.budget.step(interp, context);
    }

    fn call(&mut self, context: &mut CTX, inputs: &mut CallInputs) -> Option<CallOutcome> {
        let _ = context;
        self.traces.start_call(inputs);
        None
    }

    fn call_end(&mut self, context: &mut CTX, inputs: &CallInputs, outcome: &mut CallOutcome) {
        let _ = context;
        let _ = inputs;
        self.traces.end_call(outcome);
    }

    fn create(&mut self, context: &mut CTX, inputs: &mut CreateInputs) -> Option<CreateOutcome> {
        let _ = context;
        self.traces.start_create(inputs);
        None
    }

    fn create_end(
        &mut self,
        context: &mut CTX,
        inputs: &CreateInputs,
        outcome: &mut CreateOutcome,
    ) {
        let _ = context;
        let _ = inputs;
        self.traces.end_create(outcome);
    }

    fn selfdestruct(&mut self, contract: Address, target: Address, value: U256) {
        self.traces.record_selfdestruct(contract, target, value);
    }
}

fn render_trace_id(segments: &[u32]) -> String {
    let mut out = String::new();
    for (index, segment) in segments.iter().enumerate() {
        if index > 0 {
            out.push('_');
        }
        out.push_str(&segment.to_string());
    }
    out
}

fn trace_outcome_status(result: &InstructionResult) -> (bool, Option<String>) {
    match result {
        InstructionResult::Stop | InstructionResult::Return | InstructionResult::SelfDestruct => {
            (true, None)
        }
        InstructionResult::Revert => (false, Some("Revert".to_string())),
        other => (false, Some(format!("{other:?}"))),
    }
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
        INSTRUCTION_COUNTER_FOR_TEST.with(|counter| {
            let value = counter.get();
            let step = INSTRUCTION_COUNTER_STEP_FOR_TEST.with(|step| step.get());
            counter.set(value.saturating_add(step));
            value
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn configure_instruction_counter_for_test(start: u64, step: u64) {
    INSTRUCTION_COUNTER_FOR_TEST.with(|counter| counter.set(start));
    INSTRUCTION_COUNTER_STEP_FOR_TEST.with(|counter| counter.set(step));
}

#[cfg(not(target_arch = "wasm32"))]
pub fn configure_instruction_budget_tripped_for_test(tripped: bool) {
    INSTRUCTION_BUDGET_TRIPPED_FOR_TEST.with(|value| value.set(tripped));
}

fn instruction_budget_tripped_for_test() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        INSTRUCTION_BUDGET_TRIPPED_FOR_TEST.with(|value| value.get())
    }
}

fn validate_execution_result_sizes(output: &[u8], logs: &[LogEntry]) -> Result<(), ExecError> {
    if output.len() > MAX_RETURN_DATA {
        return Err(ExecError::ResultTooLarge);
    }
    if logs.len() > MAX_LOGS_PER_TX {
        return Err(ExecError::ResultTooLarge);
    }
    for log in logs.iter() {
        let topics = log.data.topics();
        if topics.len() > MAX_LOG_TOPICS {
            return Err(ExecError::ResultTooLarge);
        }
        let data = log.data.data.as_ref();
        if data.len() > MAX_LOG_DATA {
            return Err(ExecError::ResultTooLarge);
        }
    }
    Ok(())
}

pub(crate) fn commit_state_diff_to_db(state: StateDiff) {
    let mut db = RevmStableDb;
    db.commit(state);
}

fn collect_state_diff(state: StateDiff) -> StateDiff {
    state
}

fn add_base_fee_portion_to_recipient(state: &mut StateDiff, gas_used: u64, base_fee: u64) {
    if gas_used == 0 || base_fee == 0 {
        return;
    }
    let reward = verified_core::fee::base_fee_reward(gas_used, base_fee);
    if reward == 0 {
        return;
    }
    let recipient = FEE_RECIPIENT;
    // beneficiary未touchでも会計整合性を維持するため、既存AccountInfoをDBから取り込んで加算する。
    // これにより nonce/code_hash の上書き事故を避けつつ、base fee取りこぼしを防ぐ。
    let account = state.entry(recipient).or_insert_with(|| {
        let mut db = RevmStableDb;
        let info = db.basic(recipient).ok().flatten().unwrap_or_default();
        Account::from(info)
    });
    account.mark_touch();
    account.info.balance = account.info.balance.saturating_add(U256::from(reward));
}

fn commit_state_diff(evm: &mut impl ExecuteCommitEvm<State = StateDiff>, state: StateDiff) {
    evm.commit(state);
}

fn store_receipt_index(tx_id: TxId, block_number: u64, tx_index: u32, receipt: &ReceiptLike) {
    with_state_mut(|state| {
        let entry = TxIndexEntry {
            block_number,
            tx_index,
        };
        let entry_bytes = entry.to_bytes().into_owned();
        before_store_write_for_test("store_tx_index_entry", Some(block_number), Some(tx_id));
        let entry_ptr = state
            .blob_store
            .store_bytes(&entry_bytes)
            .unwrap_or_else(|_| {
                trap_store_err(
                    "store_tx_index_entry",
                    Some(block_number),
                    Some(tx_id),
                    "blob_store",
                );
            });
        let receipt_bytes = receipt.to_bytes().into_owned();
        before_store_write_for_test("store_receipt", Some(block_number), Some(tx_id));
        let receipt_ptr = state
            .blob_store
            .store_bytes(&receipt_bytes)
            .unwrap_or_else(|_| {
                trap_store_err(
                    "store_receipt",
                    Some(block_number),
                    Some(tx_id),
                    "blob_store",
                );
            });
        state.tx_index.insert(tx_id, entry_ptr);
        state.receipts.insert(tx_id, receipt_ptr);
    });
}

fn map_halt_reason(reason: HaltReason) -> OpHaltReason {
    match reason {
        HaltReason::OutOfGas(_) => OpHaltReason::OutOfGas,
        HaltReason::OpcodeNotFound | HaltReason::InvalidFEOpcode => OpHaltReason::InvalidOpcode,
        HaltReason::StackOverflow => OpHaltReason::StackOverflow,
        HaltReason::StackUnderflow => OpHaltReason::StackUnderflow,
        HaltReason::InvalidJump => OpHaltReason::InvalidJump,
        HaltReason::StateChangeDuringStaticCall | HaltReason::CallNotAllowedInsideStatic => {
            OpHaltReason::StateChangeDuringStaticCall
        }
        HaltReason::PrecompileError | HaltReason::PrecompileErrorWithContext(_) => {
            OpHaltReason::PrecompileError
        }
        _ => OpHaltReason::Unknown,
    }
}

fn map_tx_error_stage(error: EVMError<core::convert::Infallible, InvalidTransaction>) -> ExecError {
    match error {
        EVMError::Transaction(_) | EVMError::Header(_) => {
            ExecError::TxError(OpTransactionError::TxPrecheckFailed)
        }
        EVMError::Database(_) | EVMError::Custom(_) => {
            ExecError::TxError(OpTransactionError::TxExecutionFailed)
        }
    }
}

pub(crate) fn compute_effective_gas_price(
    max_fee: u128,
    max_priority: u128,
    base_fee: u64,
) -> Option<u64> {
    verified_core::fee::effective_gas_price(max_fee, max_priority, base_fee)
}

fn revm_log_to_receipt_log(log: revm::primitives::Log) -> LogEntry {
    let address = try_address_to_bytes(log.address).expect("revm log address must be 20 bytes");
    let topics = log.topics().iter().map(|topic| topic.0).collect::<Vec<_>>();
    let data = log.data.data.to_vec();
    log_entry_from_parts(address, topics, data)
}

#[cfg(test)]
#[path = "revm_exec_tests.rs"]
mod tests;
