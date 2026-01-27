//! どこで: Phase1のREVM実行 / 何を: TxEnvの実行とcommit / なぜ: 状態更新をEVM経由にするため

use crate::hash::keccak256;
use crate::revm_db::RevmStableDb;
use crate::tx_decode::DecodeError;
use evm_db::phase1::constants::CHAIN_ID;
use evm_db::phase1::{ReceiptLike, TxId, TxIndexEntry};
use evm_db::stable_state::with_state_mut;
use revm::context::{BlockEnv, Context};
use revm::handler::{ExecuteCommitEvm, ExecuteEvm, MainBuilder};
use revm::handler::MainnetContext;
use revm::primitives::{hardfork::SpecId, U256};
use revm::context_interface::result::ExecutionResult;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecError {
    Decode(DecodeError),
    ExecutionFailed,
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
    let db = RevmStableDb;
    let mut ctx: MainnetContext<RevmStableDb> = Context::new(db, SpecId::CANCUN);
    let mut block = BlockEnv::default();
    block.number = U256::from(block_number);
    block.timestamp = U256::from(timestamp);
    ctx.block = block;

    ctx.cfg.chain_id = CHAIN_ID;
    let mut evm = ctx.build_mainnet();
    let result = evm.transact(tx_env).map_err(|_| ExecError::ExecutionFailed)?;
    let state = result.state;
    evm.commit(state);

    let (status, gas_used, output, contract_address) = match result.result {
        ExecutionResult::Success { gas_used, output, .. } => {
            let addr = output.address().map(|a| address_to_bytes(*a));
            (1u8, gas_used, output.data().as_ref().to_vec(), addr)
        }
        ExecutionResult::Revert { gas_used, output } => (0u8, gas_used, output.to_vec(), None),
        ExecutionResult::Halt { gas_used, .. } => (0u8, gas_used, Vec::new(), None),
    };

    let return_data_hash = keccak256(&output);
    let receipt = ReceiptLike {
        tx_id,
        block_number,
        tx_index,
        status,
        gas_used,
        return_data_hash,
        contract_address,
    };

    with_state_mut(|state| {
        state.tx_index.insert(
            tx_id,
            TxIndexEntry {
                block_number,
                tx_index,
            },
        );
        state.receipts.insert(tx_id, receipt);
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
