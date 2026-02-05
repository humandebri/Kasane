//! どこで: Phase1のREVM実行 / 何を: TxEnvの実行とcommit / なぜ: 状態更新をEVM経由にするため

use crate::hash::keccak256;
use crate::revm_db::RevmStableDb;
use crate::tx_decode::DecodeError;
use evm_db::chain_data::constants::{CHAIN_ID, DEFAULT_BLOCK_GAS_LIMIT};
use evm_db::chain_data::receipt::LogEntry;
use evm_db::chain_data::{ReceiptLike, TxId, TxIndexEntry, TxKind};
use evm_db::stable_state::with_state_mut;
use ic_stable_structures::Storable;
use revm::context::{Context, TxEnv};
use revm::context_interface::result::{EVMError, ExecutionResult, HaltReason, InvalidTransaction};
use revm::database_interface::DatabaseCommit;
use revm::handler::{ExecuteCommitEvm, ExecuteEvm, MainBuilder, MainContext};
use revm::primitives::{Address, U256};

pub(crate) type StateDiff = revm::primitives::HashMap<Address, revm::state::Account>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecError {
    Decode(DecodeError),
    TxError(OpTransactionError),
    Revert,
    EvmHalt(OpHaltReason),
    ExecutionFailed,
    InvalidGasFee,
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
}

#[derive(Clone, Debug)]
pub struct BlockExecContext {
    pub block_number: u64,
    pub timestamp: u64,
    pub base_fee: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FeeBreakdown {
    l1_data_fee: u128,
    operator_fee: u128,
}

impl FeeBreakdown {
    fn total_fee(self, effective_gas_price: u64, gas_used: u64) -> u128 {
        let l2_fee = u128::from(gas_used).saturating_mul(u128::from(effective_gas_price));
        l2_fee
            .saturating_add(self.l1_data_fee)
            .saturating_add(self.operator_fee)
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
    let (outcome, _state_diff) =
        execute_tx_on(RevmStableDb, tx_id, tx_index, tx_env, exec_ctx, exec_path, true)?;
    Ok(outcome)
}

pub(crate) fn execute_tx_on<DB>(
    db: DB,
    tx_id: TxId,
    tx_index: u32,
    tx_env: TxEnv,
    exec_ctx: &BlockExecContext,
    exec_path: ExecPath,
    persist_receipt_index: bool,
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

    let mut evm = Context::mainnet()
        .with_db(db)
        .modify_cfg_chained(|cfg| {
            cfg.chain_id = CHAIN_ID;
        })
        .modify_block_chained(|block| {
            block.number = U256::from(exec_ctx.block_number);
            block.timestamp = U256::from(exec_ctx.timestamp);
            block.gas_limit = DEFAULT_BLOCK_GAS_LIMIT;
            block.basefee = exec_ctx.base_fee;
        })
        .build_mainnet();

    let result = evm.transact(tx_env).map_err(map_tx_error_stage)?;
    let state_diff = collect_state_diff(result.state);
    commit_state_diff(&mut evm, state_diff.clone());

    let (status, gas_used, output, contract_address, logs, final_status, halt_reason) =
        match result.result {
            ExecutionResult::Success {
                gas_used,
                output,
                logs,
                ..
            } => {
                let addr = output.address().map(|a| address_to_bytes(*a));
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
    };
    Ok((outcome, state_diff))
}

pub(crate) fn commit_state_diff_to_db(state: StateDiff) {
    let mut db = RevmStableDb;
    db.commit(state);
}

fn collect_state_diff(state: StateDiff) -> StateDiff {
    state
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
        let entry_ptr = state
            .blob_store
            .store_bytes(&entry_bytes)
            .unwrap_or_else(|_| panic!("blob_store: store_tx_index failed"));
        let receipt_bytes = receipt.to_bytes().into_owned();
        let receipt_ptr = state
            .blob_store
            .store_bytes(&receipt_bytes)
            .unwrap_or_else(|_| panic!("blob_store: store_receipt failed"));
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

fn map_tx_error_stage(
    error: EVMError<core::convert::Infallible, InvalidTransaction>,
) -> ExecError {
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
    if max_priority > max_fee {
        return None;
    }
    let base_fee = base_fee as u128;
    if max_fee < base_fee {
        return None;
    }
    let sum = base_fee.saturating_add(max_priority);
    let effective = if max_fee < sum { max_fee } else { sum };
    u64::try_from(effective).ok()
}

fn address_to_bytes(address: revm::primitives::Address) -> [u8; 20] {
    let mut out = [0u8; 20];
    out.copy_from_slice(address.as_ref());
    out
}

fn revm_log_to_receipt_log(log: revm::primitives::Log) -> LogEntry {
    let address = alloy_primitives::Address::from(address_to_bytes(log.address));
    let topics = log
        .topics()
        .iter()
        .map(|topic| alloy_primitives::B256::from(topic.0))
        .collect::<Vec<_>>();
    let data = alloy_primitives::Bytes::from(log.data.data.to_vec());
    LogEntry::new_unchecked(address, topics, data)
}

#[cfg(test)]
mod tests {
    use super::{compute_effective_gas_price, map_halt_reason, OpHaltReason};
    use revm::context_interface::result::{HaltReason, OutOfGasError};

    #[test]
    fn effective_price_uses_min_of_max_and_base_plus_priority() {
        let effective = compute_effective_gas_price(10, 3, 5);
        assert_eq!(effective, Some(8));
        let effective = compute_effective_gas_price(7, 7, 0);
        assert_eq!(effective, Some(7));
    }

    #[test]
    fn effective_price_rejects_invalid_fees() {
        let effective = compute_effective_gas_price(10, 11, 0);
        assert_eq!(effective, None);
        let effective = compute_effective_gas_price(9, 0, 10);
        assert_eq!(effective, None);
    }

    #[test]
    fn effective_price_handles_overflow_without_panic() {
        let effective = compute_effective_gas_price(u128::MAX, u128::MAX, u64::MAX);
        assert_eq!(effective, None);
    }

    #[test]
    fn halt_reason_mapping_covers_known_variants() {
        assert_eq!(
            map_halt_reason(HaltReason::OutOfGas(OutOfGasError::Basic)),
            OpHaltReason::OutOfGas
        );
        assert_eq!(
            map_halt_reason(HaltReason::InvalidJump),
            OpHaltReason::InvalidJump
        );
    }
}
