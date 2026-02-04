//! どこで: Phase1テスト / 何を: system tx失敗時のbackoff動作と順序保証 / なぜ: 無駄な再試行と副作用汚染を防ぐため

use evm_core::chain::{self, ChainError};
use evm_core::hash;
use evm_core::revm_exec::{ExecError, OpHaltReason};
use evm_db::chain_data::constants::DROP_CODE_EXEC;
use evm_db::chain_data::{L1BlockInfoParamsV1, L1BlockInfoSnapshotV1, TxLocKind};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
use evm_db::types::keys::{make_account_key, make_code_key};
use evm_db::types::values::{AccountVal, CodeVal};
use op_revm::constants::L1_BLOCK_CONTRACT;

#[test]
fn produce_block_backoff_counts_only_real_system_tx_failures() {
    init_stable_state();
    with_state_mut(|state| {
        let _ = state.l1_block_info_params.set(L1BlockInfoParamsV1 {
            schema_version: 1,
            spec_id: 99,
            empty_ecotone_scalars: false,
            l1_fee_overhead: 0,
            l1_base_fee_scalar: 1_000_000,
            l1_blob_base_fee_scalar: 0,
            operator_fee_scalar: 0,
            operator_fee_constant: 0,
        });
        let _ = state.l1_block_info_snapshot.set(L1BlockInfoSnapshotV1 {
            schema_version: 1,
            enabled: true,
            l1_block_number: 1,
            l1_base_fee: 1,
            l1_blob_base_fee: 0,
        });
    });

    let tx_id = chain::submit_ic_tx(vec![0x11], vec![0xaa], build_ic_tx_bytes([0x10u8; 20]))
        .expect("submit");

    let first = chain::produce_block(1);
    assert_eq!(first, Err(ChainError::ExecFailed(Some(ExecError::InvalidL1SpecId(99)))));

    // テスト決定性のため、backoff中状態を明示的に固定する。
    with_state_mut(|state| {
        let mut health = *state.system_tx_health.get();
        health.backoff_until_ts = u64::MAX;
        state.system_tx_health.set(health);
    });

    let first_health = with_state(|state| *state.system_tx_health.get());
    assert_eq!(first_health.consecutive_failures, 1);
    assert_eq!(first_health.backoff_hits, 0);

    let second = chain::produce_block(1);
    assert_eq!(second, Err(ChainError::ExecFailed(Some(ExecError::SystemTxBackoff))));

    let second_health = with_state(|state| *state.system_tx_health.get());
    assert_eq!(second_health.consecutive_failures, 1);
    assert_eq!(second_health.backoff_hits, 1);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Queued);
    let (receipt_count, tx_index_count) = with_state(|state| (state.receipts.len(), state.tx_index.len()));
    assert_eq!(receipt_count, 0);
    assert_eq!(tx_index_count, 0);
}

#[test]
fn empty_queue_returns_queue_empty_even_if_backoff_is_active() {
    init_stable_state();
    with_state_mut(|state| {
        let mut health = *state.system_tx_health.get();
        health.backoff_until_ts = u64::MAX;
        health.consecutive_failures = 7;
        state.system_tx_health.set(health);
    });

    let err = chain::produce_block(1).expect_err("queue should be empty");
    assert_eq!(err, ChainError::QueueEmpty);

    let health = with_state(|state| *state.system_tx_health.get());
    assert_eq!(health.consecutive_failures, 7);
    assert_eq!(health.backoff_hits, 0);
}

#[test]
fn sync_execute_backoff_finalizes_pending_tx() {
    init_stable_state();
    configure_l1(101, true);
    with_state_mut(|state| {
        let mut health = *state.system_tx_health.get();
        health.backoff_until_ts = u64::MAX;
        state.system_tx_health.set(health);
    });

    let caller_principal = vec![0x51];
    let caller = hash::caller_evm_from_principal(&caller_principal);
    chain::dev_mint(caller, 1_000_000_000_000_000_000).expect("mint");

    let err = chain::execute_ic_tx(caller_principal, vec![0xaa], build_ic_tx_bytes([0x21u8; 20]))
        .expect_err("backoff should reject sync path");
    assert_eq!(err, ChainError::ExecFailed(Some(ExecError::SystemTxBackoff)));

    let dropped = with_state(|state| {
        let tx_id = state
            .tx_locs
            .range(..)
            .next()
            .map(|entry| entry.value())
            .expect("tx_loc exists");
        (
            tx_id.kind,
            tx_id.drop_code,
            state.pending_meta_by_tx_id.len(),
            state.ready_key_by_tx_id.len(),
            state.pending_by_sender_nonce.len(),
        )
    });
    assert_eq!(dropped.0, TxLocKind::Dropped);
    assert_eq!(dropped.1, DROP_CODE_EXEC);
    assert_eq!(dropped.2, 0);
    assert_eq!(dropped.3, 0);
    assert_eq!(dropped.4, 0);
}

#[test]
fn produce_block_revert_failure_keeps_consensus_state_unchanged() {
    init_stable_state();
    configure_l1(101, true);
    install_l1_system_code(&[0x60, 0x00, 0x60, 0x00, 0xfd]); // REVERT(0,0)

    let tx_id = chain::submit_ic_tx(vec![0x61], vec![0xaa], build_ic_tx_bytes([0x31u8; 20]))
        .expect("submit");
    let before = snapshot_consensus_state(&tx_id);

    let err = chain::produce_block(1).expect_err("system tx revert should fail block");
    assert_eq!(err, ChainError::ExecFailed(Some(ExecError::Revert)));

    let after = snapshot_consensus_state(&tx_id);
    assert_eq!(before, after);
}

#[test]
fn produce_block_halt_failure_keeps_consensus_state_unchanged() {
    init_stable_state();
    configure_l1(101, true);
    install_l1_system_code(&[0xfe]); // INVALID -> Halt

    let tx_id = chain::submit_ic_tx(vec![0x71], vec![0xaa], build_ic_tx_bytes([0x41u8; 20]))
        .expect("submit");
    let before = snapshot_consensus_state(&tx_id);

    let err = chain::produce_block(1).expect_err("system tx halt should fail block");
    assert_eq!(
        err,
        ChainError::ExecFailed(Some(ExecError::EvmHalt(OpHaltReason::InvalidOpcode)))
    );

    let after = snapshot_consensus_state(&tx_id);
    assert_eq!(before, after);
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct ConsensusSnapshot {
    head_number: u64,
    head_hash: [u8; 32],
    ready_len: u64,
    ready_key_len: u64,
    pending_by_sender_nonce_len: u64,
    pending_meta_len: u64,
    pending_min_len: u64,
    tx_locs_len: u64,
    receipts_len: u64,
    tx_index_len: u64,
    loc_kind: TxLocKind,
}

fn snapshot_consensus_state(tx_id: &evm_db::chain_data::TxId) -> ConsensusSnapshot {
    with_state(|state| {
        let head = *state.head.get();
        let loc = state.tx_locs.get(tx_id).expect("tx_loc");
        ConsensusSnapshot {
            head_number: head.number,
            head_hash: head.block_hash,
            ready_len: state.ready_queue.len(),
            ready_key_len: state.ready_key_by_tx_id.len(),
            pending_by_sender_nonce_len: state.pending_by_sender_nonce.len(),
            pending_meta_len: state.pending_meta_by_tx_id.len(),
            pending_min_len: state.pending_min_nonce.len(),
            tx_locs_len: state.tx_locs.len(),
            receipts_len: state.receipts.len(),
            tx_index_len: state.tx_index.len(),
            loc_kind: loc.kind,
        }
    })
}

fn configure_l1(spec_id: u8, enabled: bool) {
    with_state_mut(|state| {
        let _ = state.l1_block_info_params.set(L1BlockInfoParamsV1 {
            schema_version: 1,
            spec_id,
            empty_ecotone_scalars: false,
            l1_fee_overhead: 0,
            l1_base_fee_scalar: 1_000_000,
            l1_blob_base_fee_scalar: 0,
            operator_fee_scalar: 0,
            operator_fee_constant: 0,
        });
        let _ = state.l1_block_info_snapshot.set(L1BlockInfoSnapshotV1 {
            schema_version: 1,
            enabled,
            l1_block_number: 1,
            l1_base_fee: 1,
            l1_blob_base_fee: 0,
        });
    });
}

fn install_l1_system_code(code: &[u8]) {
    let code_hash = hash::keccak256(code);
    let mut addr = [0u8; 20];
    addr.copy_from_slice(L1_BLOCK_CONTRACT.as_ref());
    with_state_mut(|state| {
        state
            .accounts
            .insert(make_account_key(addr), AccountVal::from_parts(0, [0u8; 32], code_hash));
        state.codes.insert(make_code_key(code_hash), CodeVal(code.to_vec()));
    });
}

fn build_ic_tx_bytes(to: [u8; 20]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(2u8);
    out.extend_from_slice(&to);
    out.extend_from_slice(&[0u8; 32]);
    out.extend_from_slice(&50_000u64.to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&2_000_000_000u128.to_be_bytes());
    out.extend_from_slice(&1_000_000_000u128.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}
