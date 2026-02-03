//! どこで: Phase1のREVM実行 / 何を: TxEnvの実行とcommit / なぜ: 状態更新をEVM経由にするため

use crate::hash::keccak256;
use crate::revm_db::RevmStableDb;
use crate::tx_decode::DecodeError;
use evm_db::chain_data::constants::{CHAIN_ID, DEFAULT_BLOCK_GAS_LIMIT};
use evm_db::chain_data::receipt::LogEntry;
use evm_db::chain_data::{
    L1BlockInfoParamsV1, L1BlockInfoSnapshotV1, ReceiptLike, TxId, TxIndexEntry, TxKind,
};
use evm_db::stable_state::with_state_mut;
use ic_stable_structures::Storable;
use op_revm::constants::L1_BLOCK_CONTRACT;
use op_revm::transaction::deposit::DEPOSIT_TRANSACTION_TYPE;
use op_revm::{DefaultOp, L1BlockInfo, OpBuilder, OpContext, OpSpecId, OpTransaction};
use revm::context::{BlockEnv, TxEnv};
use revm::context_interface::result::ExecutionResult;
use revm::handler::{ExecuteCommitEvm, ExecuteEvm, SystemCallEvm};
use revm::primitives::{Address, B256, Bytes, U256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecError {
    Decode(DecodeError),
    FailedDeposit,
    SystemTxRejected,
    EvmHalt(String),
    InvalidL1SpecId(u8),
    ExecutionFailed,
    InvalidGasFee,
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
    pub l1_fee_fallback_used: bool,
}

#[derive(Clone, Debug)]
pub struct BlockExecContext {
    pub block_number: u64,
    pub timestamp: u64,
    pub base_fee: u64,
    pub l1_params: L1BlockInfoParamsV1,
    pub l1_snapshot: L1BlockInfoSnapshotV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FeeBreakdown {
    l1_data_fee: u128,
    operator_fee: u128,
    fallback_used: bool,
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
    tx_kind: TxKind,
    raw_tx: &[u8],
    tx_env: revm::context::TxEnv,
    exec_ctx: &BlockExecContext,
    exec_path: ExecPath,
) -> Result<ExecOutcome, ExecError> {
    if exec_path == ExecPath::SystemTx {
        return Err(ExecError::SystemTxRejected);
    }
    let spec = op_spec_id_from_u8(exec_ctx.l1_params.spec_id).map_err(ExecError::InvalidL1SpecId)?;
    let effective_gas_price = compute_effective_gas_price(
        tx_env.gas_price,
        tx_env.gas_priority_fee.unwrap_or(0),
        exec_ctx.base_fee,
    )
    .ok_or(ExecError::InvalidGasFee)?;
    let mut evm = build_op_context(exec_ctx, spec).build_op();
    let op_tx = build_op_transaction(tx_kind, raw_tx, tx_env, exec_path)?;
    let result = evm.transact(op_tx).map_err(|_| ExecError::ExecutionFailed)?;
    let state_diff = collect_state_diff(result.state);
    commit_state_diff(&mut evm, state_diff);

    let (status, gas_used, output, contract_address, logs, final_status) = match result.result {
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
            (
                1u8,
                gas_used,
                output.data().as_ref().to_vec(),
                addr,
                mapped,
                "Success".to_string(),
            )
        }
        ExecutionResult::Revert { gas_used, output } => {
            (0u8, gas_used, output.to_vec(), None, Vec::new(), "Revert".to_string())
        }
        ExecutionResult::Halt {
            gas_used,
            reason,
        } => (
            0u8,
            gas_used,
            Vec::new(),
            None,
            Vec::new(),
            format!("Halt:{reason:?}"),
        ),
    };
    let fee_breakdown = compute_fee_breakdown(raw_tx, gas_used, spec, exec_ctx);

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

    // User tx only: system tx accounting must not create receipt/index entries.
    store_receipt_index(tx_id, exec_ctx.block_number, tx_index, &receipt);

    Ok(ExecOutcome {
        tx_id,
        tx_index,
        receipt,
        return_data: output,
        final_status,
        l1_fee_fallback_used: fee_breakdown.fallback_used,
    })
}

pub fn execute_l1_block_info_system_tx(exec_ctx: &BlockExecContext) -> Result<(), ExecError> {
    let spec = op_spec_id_from_u8(exec_ctx.l1_params.spec_id).map_err(ExecError::InvalidL1SpecId)?;
    let mut evm = build_op_context(exec_ctx, spec).build_op();
    let call_data = build_l1blockinfo_calldata(spec, exec_ctx)?;
    let result = evm
        .system_call(L1_BLOCK_CONTRACT, call_data)
        .map_err(|_| ExecError::SystemTxRejected)?;
    match result.result {
        ExecutionResult::Success { .. } => {}
        ExecutionResult::Revert { .. } => return Err(ExecError::SystemTxRejected),
        ExecutionResult::Halt { reason, .. } => return Err(ExecError::EvmHalt(format!("{reason:?}"))),
    }
    let state_diff = collect_state_diff(result.state);
    commit_state_diff(&mut evm, state_diff);
    Ok(())
}

fn build_l1blockinfo_calldata(
    spec: OpSpecId,
    exec_ctx: &BlockExecContext,
) -> Result<Bytes, ExecError> {
    if spec.is_enabled_in(OpSpecId::ECOTONE) {
        Ok(build_l1blockinfo_calldata_v2(exec_ctx))
    } else {
        Ok(build_l1blockinfo_calldata_v1(exec_ctx))
    }
}

fn build_l1blockinfo_calldata_v1(exec_ctx: &BlockExecContext) -> Bytes {
    // setL1BlockValues(uint64,uint64,uint256,bytes32,uint64,bytes32,uint256,uint256)
    let selector_hash = keccak256(
        b"setL1BlockValues(uint64,uint64,uint256,bytes32,uint64,bytes32,uint256,uint256)",
    );
    let mut out = Vec::with_capacity(4 + 8 * 32);
    out.extend_from_slice(&selector_hash[..4]);
    push_u64_word(&mut out, exec_ctx.l1_snapshot.l2_block_number);
    push_u64_word(&mut out, exec_ctx.timestamp);
    push_u256_word(&mut out, U256::from(exec_ctx.l1_snapshot.l1_base_fee));
    push_b256_word(&mut out, B256::ZERO);
    push_u64_word(&mut out, 0);
    push_b256_word(&mut out, B256::ZERO);
    push_u256_word(&mut out, U256::from(exec_ctx.l1_params.l1_fee_overhead));
    push_u256_word(&mut out, U256::from(exec_ctx.l1_params.l1_base_fee_scalar));
    Bytes::from(out)
}

fn build_l1blockinfo_calldata_v2(exec_ctx: &BlockExecContext) -> Bytes {
    // setL1BlockValuesEcotone(uint32,uint32,uint64,uint64,uint64,uint256,uint256,bytes32,bytes32)
    let selector_hash = keccak256(
        b"setL1BlockValuesEcotone(uint32,uint32,uint64,uint64,uint64,uint256,uint256,bytes32,bytes32)",
    );
    let mut out = Vec::with_capacity(4 + 9 * 32);
    out.extend_from_slice(&selector_hash[..4]);
    push_u32_word(
        &mut out,
        u32::try_from(exec_ctx.l1_params.l1_base_fee_scalar).unwrap_or(u32::MAX),
    );
    push_u32_word(
        &mut out,
        u32::try_from(exec_ctx.l1_params.l1_blob_base_fee_scalar).unwrap_or(u32::MAX),
    );
    push_u64_word(&mut out, 0); // sequence number
    push_u64_word(&mut out, exec_ctx.timestamp);
    push_u64_word(&mut out, exec_ctx.l1_snapshot.l2_block_number);
    push_u256_word(&mut out, U256::from(exec_ctx.l1_snapshot.l1_base_fee));
    push_u256_word(&mut out, U256::from(exec_ctx.l1_snapshot.l1_blob_base_fee));
    push_b256_word(&mut out, B256::ZERO);
    push_b256_word(&mut out, B256::ZERO);
    Bytes::from(out)
}

fn push_u32_word(out: &mut Vec<u8>, value: u32) {
    let mut word = [0u8; 32];
    word[28..32].copy_from_slice(&value.to_be_bytes());
    out.extend_from_slice(&word);
}

fn push_u64_word(out: &mut Vec<u8>, value: u64) {
    let mut word = [0u8; 32];
    word[24..32].copy_from_slice(&value.to_be_bytes());
    out.extend_from_slice(&word);
}

fn push_u256_word(out: &mut Vec<u8>, value: U256) {
    out.extend_from_slice(&value.to_be_bytes::<32>());
}

fn push_b256_word(out: &mut Vec<u8>, value: B256) {
    out.extend_from_slice(value.as_ref());
}

fn build_op_context(exec_ctx: &BlockExecContext, spec: OpSpecId) -> OpContext<RevmStableDb> {
    let mut ctx: OpContext<RevmStableDb> = OpContext::op().with_db(RevmStableDb);
    ctx.block = BlockEnv {
        number: U256::from(exec_ctx.block_number),
        timestamp: U256::from(exec_ctx.timestamp),
        gas_limit: DEFAULT_BLOCK_GAS_LIMIT,
        basefee: exec_ctx.base_fee,
        ..Default::default()
    };
    ctx.cfg.chain_id = CHAIN_ID;
    ctx.cfg.spec = spec;
    ctx.chain = l1_block_info_from_context(exec_ctx);
    ctx
}

fn collect_state_diff(
    state: revm::primitives::HashMap<Address, revm::state::Account>,
) -> revm::primitives::HashMap<Address, revm::state::Account> {
    state
}

fn commit_state_diff(
    evm: &mut impl ExecuteCommitEvm<State = revm::primitives::HashMap<Address, revm::state::Account>>,
    state: revm::primitives::HashMap<Address, revm::state::Account>,
) {
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

fn build_op_transaction(
    tx_kind: TxKind,
    raw_tx: &[u8],
    tx_env: TxEnv,
    exec_path: ExecPath,
) -> Result<OpTransaction<TxEnv>, ExecError> {
    let base = TxEnv::builder()
        .tx_type(Some(tx_env.tx_type))
        .caller(tx_env.caller)
        .gas_limit(tx_env.gas_limit)
        .gas_price(tx_env.gas_price)
        .kind(tx_env.kind)
        .value(tx_env.value)
        .data(tx_env.data)
        .nonce(tx_env.nonce)
        .chain_id(tx_env.chain_id)
        .access_list(tx_env.access_list)
        .gas_priority_fee(tx_env.gas_priority_fee)
        .blob_hashes(tx_env.blob_hashes)
        .max_fee_per_blob_gas(tx_env.max_fee_per_blob_gas)
        .authorization_list(tx_env.authorization_list);

    if tx_kind == TxKind::OpDeposit || tx_env.tx_type == DEPOSIT_TRANSACTION_TYPE {
        return Err(match exec_path {
            ExecPath::UserTx => ExecError::FailedDeposit,
            ExecPath::SystemTx => ExecError::SystemTxRejected,
        });
    }

    OpTransaction::builder()
        .base(base)
        .enveloped_tx(Some(raw_tx.to_vec().into()))
        .build()
        .map_err(|_| ExecError::ExecutionFailed)
}

fn compute_fee_breakdown(
    raw_tx: &[u8],
    gas_used: u64,
    spec: OpSpecId,
    exec_ctx: &BlockExecContext,
) -> FeeBreakdown {
    if !exec_ctx.l1_snapshot.enabled {
        return FeeBreakdown {
            l1_data_fee: 0,
            operator_fee: 0,
            fallback_used: true,
        };
    }
    let mut l1_block_info = l1_block_info_from_context(exec_ctx);
    let l1_data_fee = saturating_u256_to_u128(l1_block_info.calculate_tx_l1_cost(raw_tx, spec));
    let operator_fee = if spec.is_enabled_in(OpSpecId::ISTHMUS) {
        saturating_u256_to_u128(
            l1_block_info.operator_fee_charge(raw_tx, U256::from(gas_used), spec),
        )
    } else {
        0
    };
    FeeBreakdown {
        l1_data_fee,
        operator_fee,
        fallback_used: false,
    }
}

fn l1_block_info_from_context(exec_ctx: &BlockExecContext) -> L1BlockInfo {
    L1BlockInfo {
        l2_block: Some(U256::from(exec_ctx.l1_snapshot.l2_block_number)),
        l1_base_fee: U256::from(exec_ctx.l1_snapshot.l1_base_fee),
        l1_fee_overhead: Some(U256::from(exec_ctx.l1_params.l1_fee_overhead)),
        l1_base_fee_scalar: U256::from(exec_ctx.l1_params.l1_base_fee_scalar),
        l1_blob_base_fee: Some(U256::from(exec_ctx.l1_snapshot.l1_blob_base_fee)),
        l1_blob_base_fee_scalar: Some(U256::from(exec_ctx.l1_params.l1_blob_base_fee_scalar)),
        operator_fee_scalar: Some(U256::from(exec_ctx.l1_params.operator_fee_scalar)),
        operator_fee_constant: Some(U256::from(exec_ctx.l1_params.operator_fee_constant)),
        da_footprint_gas_scalar: None,
        empty_ecotone_scalars: exec_ctx.l1_params.empty_ecotone_scalars,
        tx_l1_cost: None,
    }
}

fn op_spec_id_from_u8(value: u8) -> Result<OpSpecId, u8> {
    match value {
        100 => Ok(OpSpecId::BEDROCK),
        101 => Ok(OpSpecId::REGOLITH),
        102 => Ok(OpSpecId::CANYON),
        103 => Ok(OpSpecId::ECOTONE),
        104 => Ok(OpSpecId::FJORD),
        105 => Ok(OpSpecId::GRANITE),
        106 => Ok(OpSpecId::HOLOCENE),
        107 => Ok(OpSpecId::ISTHMUS),
        108 => Ok(OpSpecId::JOVIAN),
        109 => Ok(OpSpecId::INTEROP),
        110 => Ok(OpSpecId::OSAKA),
        _ => Err(value),
    }
}

fn saturating_u256_to_u128(value: U256) -> u128 {
    u128::try_from(value).unwrap_or(u128::MAX)
}

fn address_to_bytes(address: revm::primitives::Address) -> [u8; 20] {
    let mut out = [0u8; 20];
    out.copy_from_slice(address.as_ref());
    out
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
    use super::{
        build_l1blockinfo_calldata, build_op_transaction, compute_effective_gas_price,
        compute_fee_breakdown, execute_tx, op_spec_id_from_u8, BlockExecContext, ExecError,
        ExecPath, FeeBreakdown,
    };
    use op_revm::OpSpecId;
    use op_revm::transaction::deposit::DEPOSIT_TRANSACTION_TYPE;
    use revm::context::TxEnv;
    use evm_db::chain_data::{L1BlockInfoParamsV1, L1BlockInfoSnapshotV1, TxId, TxKind};

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
    fn total_fee_uses_fixed_formula() {
        let fee = FeeBreakdown {
            l1_data_fee: 3,
            operator_fee: 4,
            fallback_used: false,
        };
        assert_eq!(fee.total_fee(2, 5), 17);
    }

    #[test]
    fn l1_fee_fallback_is_used_when_snapshot_disabled() {
        let ctx = BlockExecContext {
            block_number: 1,
            timestamp: 1,
            base_fee: 1,
            l1_params: L1BlockInfoParamsV1::new(),
            l1_snapshot: L1BlockInfoSnapshotV1::new(),
        };
        let out = compute_fee_breakdown(&[0x02, 0x01], 21_000, OpSpecId::REGOLITH, &ctx);
        assert!(out.fallback_used);
        assert_eq!(out.l1_data_fee, 0);
        assert_eq!(out.operator_fee, 0);
    }

    #[test]
    fn invalid_spec_id_is_rejected() {
        assert_eq!(op_spec_id_from_u8(99), Err(99));
        assert_eq!(op_spec_id_from_u8(111), Err(111));
    }

    #[test]
    fn all_supported_spec_ids_are_mapped() {
        for value in 100u8..=110u8 {
            assert!(op_spec_id_from_u8(value).is_ok(), "spec_id={value}");
        }
    }

    #[test]
    fn l1_block_info_call_data_switches_at_ecotone_boundary() {
        let ctx = BlockExecContext {
            block_number: 11,
            timestamp: 22,
            base_fee: 33,
            l1_params: L1BlockInfoParamsV1 {
                schema_version: 1,
                spec_id: 102,
                empty_ecotone_scalars: false,
                l1_fee_overhead: 44,
                l1_base_fee_scalar: 55,
                l1_blob_base_fee_scalar: 66,
                operator_fee_scalar: 0,
                operator_fee_constant: 0,
            },
            l1_snapshot: L1BlockInfoSnapshotV1 {
                schema_version: 1,
                enabled: true,
                l2_block_number: 66,
                l1_base_fee: 77,
                l1_blob_base_fee: 88,
            },
        };
        let pre = build_l1blockinfo_calldata(OpSpecId::CANYON, &ctx).expect("pre-ecotone");
        let post = build_l1blockinfo_calldata(OpSpecId::ECOTONE, &ctx).expect("post-ecotone");
        assert_eq!(pre.len(), 4 + 8 * 32);
        assert_eq!(post.len(), 4 + 9 * 32);
    }

    #[test]
    fn deposit_path_returns_fixed_error_tag() {
        let tx_env = TxEnv::builder()
            .tx_type(Some(DEPOSIT_TRANSACTION_TYPE))
            .gas_limit(21_000)
            .build()
            .expect("tx env");
        let err = build_op_transaction(
            TxKind::OpDeposit,
            &[DEPOSIT_TRANSACTION_TYPE],
            tx_env,
            ExecPath::UserTx,
        )
        .expect_err("deposit should fail");
        assert_eq!(err, ExecError::FailedDeposit);
    }

    #[test]
    fn system_exec_path_is_rejected_before_accounting() {
        let ctx = BlockExecContext {
            block_number: 1,
            timestamp: 1,
            base_fee: 1,
            l1_params: L1BlockInfoParamsV1::new(),
            l1_snapshot: L1BlockInfoSnapshotV1::new(),
        };
        let tx_env = TxEnv::builder().gas_limit(21_000).build().expect("tx env");
        let out = execute_tx(
            TxId([0u8; 32]),
            0,
            TxKind::EthSigned,
            &[0x02],
            tx_env,
            &ctx,
            ExecPath::SystemTx,
        );
        assert!(matches!(out, Err(ExecError::SystemTxRejected)));
    }

    #[test]
    fn l1_block_info_call_data_has_expected_shape() {
        let ctx = BlockExecContext {
            block_number: 11,
            timestamp: 22,
            base_fee: 33,
            l1_params: L1BlockInfoParamsV1 {
                schema_version: 1,
                spec_id: 101,
                empty_ecotone_scalars: false,
                l1_fee_overhead: 44,
                l1_base_fee_scalar: 55,
                l1_blob_base_fee_scalar: 0,
                operator_fee_scalar: 0,
                operator_fee_constant: 0,
            },
            l1_snapshot: L1BlockInfoSnapshotV1 {
                schema_version: 1,
                enabled: true,
                l2_block_number: 66,
                l1_base_fee: 77,
                l1_blob_base_fee: 0,
            },
        };
        let data = build_l1blockinfo_calldata(OpSpecId::REGOLITH, &ctx).expect("calldata");
        assert_eq!(data.len(), 4 + 8 * 32);
        let selector = &crate::hash::keccak256(
            b"setL1BlockValues(uint64,uint64,uint256,bytes32,uint64,bytes32,uint256,uint256)",
        )[..4];
        assert_eq!(&data[..4], selector);
    }

    #[test]
    fn l1_block_info_call_data_v2_has_expected_shape() {
        let ctx = BlockExecContext {
            block_number: 11,
            timestamp: 22,
            base_fee: 33,
            l1_params: L1BlockInfoParamsV1 {
                schema_version: 1,
                spec_id: 103,
                empty_ecotone_scalars: false,
                l1_fee_overhead: 44,
                l1_base_fee_scalar: 55,
                l1_blob_base_fee_scalar: 66,
                operator_fee_scalar: 0,
                operator_fee_constant: 0,
            },
            l1_snapshot: L1BlockInfoSnapshotV1 {
                schema_version: 1,
                enabled: true,
                l2_block_number: 66,
                l1_base_fee: 77,
                l1_blob_base_fee: 88,
            },
        };
        let data = build_l1blockinfo_calldata(OpSpecId::ECOTONE, &ctx).expect("calldata");
        assert_eq!(data.len(), 4 + 9 * 32);
        let selector = &crate::hash::keccak256(
            b"setL1BlockValuesEcotone(uint32,uint32,uint64,uint64,uint64,uint256,uint256,bytes32,bytes32)",
        )[..4];
        assert_eq!(&data[..4], selector);
    }
}
