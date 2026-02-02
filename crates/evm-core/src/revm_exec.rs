//! どこで: Phase1のREVM実行 / 何を: TxEnvの実行とcommit / なぜ: 状態更新をEVM経由にするため

use crate::hash::keccak256;
use crate::revm_db::RevmStableDb;
use crate::tx_decode::DecodeError;
use evm_db::chain_data::constants::{CHAIN_ID, DEFAULT_BLOCK_GAS_LIMIT};
use evm_db::chain_data::receipt::LogEntry;
use evm_db::chain_data::{ReceiptLike, TxId, TxIndexEntry};
use evm_db::stable_state::{with_state, with_state_mut};
use ic_stable_structures::Storable;
use revm::context::{BlockEnv, Context};
use revm::handler::{ExecuteCommitEvm, ExecuteEvm, MainBuilder};
use revm::handler::MainnetContext;
use revm::primitives::{hardfork::SpecId, Address, B256, U256};
use revm::context_interface::result::ExecutionResult;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecError {
    Decode(DecodeError),
    ExecutionFailed(String),
    InvalidGasFee,
}

pub struct ExecOutcome {
    pub tx_id: TxId,
    pub tx_index: u32,
    pub receipt: ReceiptLike,
    pub return_data: Vec<u8>,
    pub state_change_hash: [u8; 32],
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
    let block = BlockEnv {
        number: U256::from(block_number),
        timestamp: U256::from(timestamp),
        gas_limit: DEFAULT_BLOCK_GAS_LIMIT,
        basefee: base_fee,
        ..Default::default()
    };
    ctx.block = block;

    ctx.cfg.chain_id = CHAIN_ID;
    let mut evm = ctx.build_mainnet();
    let result = evm
        .transact(tx_env)
        .map_err(|err| ExecError::ExecutionFailed(format!("{:?}", err)))?;
    let state = result.state;
    let state_change_hash = compute_tx_change_hash(&state);
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

    Ok(ExecOutcome {
        tx_id,
        tx_index,
        receipt,
        return_data: output,
        state_change_hash,
    })
}

fn address_to_bytes(address: revm::primitives::Address) -> [u8; 20] {
    let mut out = [0u8; 20];
    out.copy_from_slice(address.as_ref());
    out
}

fn b256_to_bytes(hash: B256) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(hash.as_ref());
    out
}

fn u256_to_bytes(value: U256) -> [u8; 32] {
    value.to_be_bytes()
}

fn compute_tx_change_hash(state: &revm::primitives::HashMap<Address, revm::state::Account>) -> [u8; 32] {
    let mut accounts: Vec<([u8; 20], &revm::state::Account)> = Vec::new();
    for (address, account) in state.iter() {
        accounts.push((address_to_bytes(*address), account));
    }
    accounts.sort_by(|left, right| left.0.cmp(&right.0));

    let mut buf = Vec::new();
    buf.extend_from_slice(b"ic-evm:tx-change:v1");

    for (address_bytes, account) in accounts.into_iter() {
        buf.extend_from_slice(&address_bytes);
        let mut flags = 0u8;
        if account.is_selfdestructed() {
            flags |= 0x01;
        }
        if account.is_empty() && account.is_touched() {
            flags |= 0x02;
        }
        if account.info.code.is_some() {
            flags |= 0x04;
        }
        buf.push(flags);
        buf.extend_from_slice(&account.info.nonce.to_be_bytes());
        buf.extend_from_slice(&u256_to_bytes(account.info.balance));
        buf.extend_from_slice(&b256_to_bytes(account.info.code_hash));
        if let Some(code) = account.info.code.as_ref() {
            let bytes = code.original_byte_slice();
            let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
            buf.extend_from_slice(&len.to_be_bytes());
            buf.extend_from_slice(bytes);
        } else {
            buf.extend_from_slice(&0u32.to_be_bytes());
        }

        let mut slots: Vec<([u8; 32], [u8; 32])> = Vec::new();
        for (slot, entry) in account.changed_storage_slots() {
            let slot_bytes = u256_to_bytes(*slot);
            let value_bytes = u256_to_bytes(entry.present_value);
            slots.push((slot_bytes, value_bytes));
        }
        slots.sort_by(|left, right| left.0.cmp(&right.0));
        let count = u32::try_from(slots.len()).unwrap_or(u32::MAX);
        buf.extend_from_slice(&count.to_be_bytes());
        for (slot_bytes, value_bytes) in slots.into_iter() {
            buf.extend_from_slice(&slot_bytes);
            buf.extend_from_slice(&value_bytes);
        }
    }
    keccak256(&buf)
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
