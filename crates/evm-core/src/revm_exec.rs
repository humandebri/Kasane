//! どこで: Phase1のREVM実行 / 何を: TxEnvの実行とcommit / なぜ: 状態更新をEVM経由にするため

use crate::hash::keccak256;
use crate::revm_db::RevmStableDb;
use crate::tx_decode::DecodeError;
use evm_db::chain_data::constants::CHAIN_ID;
use evm_db::chain_data::receipt::LogEntry;
use evm_db::chain_data::{ReceiptLike, TxId, TxIndexEntry};
use evm_db::stable_state::{with_state, with_state_mut};
use revm::context::{BlockEnv, Context};
use revm::handler::{ExecuteCommitEvm, ExecuteEvm, MainBuilder};
use revm::handler::MainnetContext;
use revm::primitives::{hardfork::SpecId, U256};
use revm::context_interface::result::ExecutionResult;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecError {
    Decode(DecodeError),
    ExecutionFailed,
    InvalidGasFee,
}

pub struct ExecOutcome {
    pub tx_id: TxId,
    pub tx_index: u32,
    pub receipt: ReceiptLike,
    pub return_data: Vec<u8>,
}

pub fn execute_tx(
    tx_id: TxId,
    tx_index: u32,
    tx_env: revm::context::TxEnv,
    block_number: u64,
    timestamp: u64,
) -> Result<ExecOutcome, ExecError> {
    let base_fee = with_state(|state| state.chain_state.get().base_fee);
    let max_fee = tx_env.gas_price;
    let max_priority = tx_env.gas_priority_fee.unwrap_or(0);
    let effective_gas_price =
        compute_effective_gas_price(max_fee, max_priority, base_fee).ok_or(ExecError::InvalidGasFee)?;
    let db = RevmStableDb;
    let mut ctx: MainnetContext<RevmStableDb> = Context::new(db, SpecId::CANCUN);
    let mut block = BlockEnv::default();
    block.number = U256::from(block_number);
    block.timestamp = U256::from(timestamp);
    block.basefee = base_fee;
    ctx.block = block;

    ctx.cfg.chain_id = CHAIN_ID;
    let mut evm = ctx.build_mainnet();
    let result = evm.transact(tx_env).map_err(|_| ExecError::ExecutionFailed)?;
    let state = result.state;
    evm.commit(state);

    let (status, gas_used, output, contract_address, logs) = match result.result {
        ExecutionResult::Success {
            gas_used,
            output,
            logs,
            ..
        } => {
            let addr = output.address().map(|a| address_to_bytes(*a));
            let mapped = logs
                .into_iter()
                .map(|log| LogEntry {
                    address: address_to_bytes(log.address),
                    topics: log.topics().iter().map(|t| t.0).collect(),
                    data: log.data.data.to_vec(),
                })
                .collect::<Vec<_>>();
            (1u8, gas_used, output.data().as_ref().to_vec(), addr, mapped)
        }
        ExecutionResult::Revert { gas_used, output } => {
            (0u8, gas_used, output.to_vec(), None, Vec::new())
        }
        ExecutionResult::Halt { gas_used, .. } => (0u8, gas_used, Vec::new(), None, Vec::new()),
    };

    let return_data_hash = keccak256(&output);
    let receipt = ReceiptLike {
        tx_id,
        block_number,
        tx_index,
        status,
        gas_used,
        effective_gas_price,
        return_data_hash,
        return_data: output.clone(),
        contract_address,
        logs,
    };

    with_state_mut(|state| {
        state.tx_index.insert(
            tx_id,
            TxIndexEntry {
                block_number,
                tx_index,
            },
        );
        state.receipts.insert(tx_id, receipt.clone());
    });

    Ok(ExecOutcome {
        tx_id,
        tx_index,
        receipt,
        return_data: output,
    })
}

fn address_to_bytes(address: revm::primitives::Address) -> [u8; 20] {
    let mut out = [0u8; 20];
    out.copy_from_slice(address.as_ref());
    out
}

fn compute_effective_gas_price(
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
    let sum = base_fee.checked_add(max_priority).unwrap_or(u128::MAX);
    let effective = if max_fee < sum { max_fee } else { sum };
    u64::try_from(effective).ok()
}

#[cfg(test)]
mod tests {
    use super::compute_effective_gas_price;

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
}
