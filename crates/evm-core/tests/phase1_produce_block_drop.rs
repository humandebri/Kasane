//! どこで: Phase1テスト / 何を: produce_block の drop_code / なぜ: 失敗理由の可視化を固定するため

use evm_core::chain::{self, ChainError};
use evm_db::chain_data::constants::{
    DROP_CODE_BLOCK_GAS_EXCEEDED, DROP_CODE_CALLER_MISSING, DROP_CODE_DECODE,
    DROP_CODE_EXEC_PRECHECK, DROP_CODE_MISSING,
};
use evm_db::chain_data::{
    ReadyKey, SenderKey, SenderNonceKey, StoredTxBytes, TxId, TxKind, TxLoc, TxLocKind,
};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
use evm_db::types::keys::make_account_key;
use evm_db::types::values::AccountVal;

const TEST_MAX_FEE_PER_GAS: u128 = 500_000_000_000;
const TEST_MAX_PRIORITY_FEE_PER_GAS: u128 = 250_000_000_000;

#[test]
fn produce_block_marks_decode_drop() {
    init_stable_state();

    let tx_id = TxId([0x10u8; 32]);
    let envelope = StoredTxBytes::new_with_fees(
        tx_id,
        TxKind::EthSigned,
        vec![0x01],
        None,
        Vec::new(),
        Vec::new(),
        0,
        0,
        false,
    );
    let sender = [0x11u8; 20];
    let pending_key = SenderNonceKey::new(sender, 0);
    with_state_mut(|state| {
        state.tx_store.insert(tx_id, envelope);
        state.tx_locs.insert(tx_id, TxLoc::queued(0));
        state.pending_by_sender_nonce.insert(pending_key, tx_id);
        state.pending_meta_by_tx_id.insert(tx_id, pending_key);
        state.pending_min_nonce.insert(SenderKey::new(sender), 0);
        let key = ReadyKey::new(1, 0, 0, tx_id.0);
        state.ready_queue.insert(key, tx_id);
        state.ready_key_by_tx_id.insert(tx_id, key);
    });

    let err = chain::produce_block(1).expect_err("produce_block should fail");
    assert_eq!(err, ChainError::NoExecutableTx);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Dropped);
    assert_eq!(loc.drop_code, DROP_CODE_DECODE);

    with_state(|state| {
        assert!(state.tx_store.get(&tx_id).is_none());
        assert!(state.ready_key_by_tx_id.get(&tx_id).is_none());
        assert!(state.pending_by_sender_nonce.get(&pending_key).is_none());
    });
}

#[test]
fn produce_block_marks_missing_envelope() {
    init_stable_state();

    let tx_id = TxId([0x22u8; 32]);
    let sender = [0x33u8; 20];
    let pending_key = SenderNonceKey::new(sender, 0);
    with_state_mut(|state| {
        state.tx_locs.insert(tx_id, TxLoc::queued(0));
        state.pending_by_sender_nonce.insert(pending_key, tx_id);
        state.pending_meta_by_tx_id.insert(tx_id, pending_key);
        state.pending_min_nonce.insert(SenderKey::new(sender), 0);
        let key = ReadyKey::new(1, 0, 0, tx_id.0);
        state.ready_queue.insert(key, tx_id);
        state.ready_key_by_tx_id.insert(tx_id, key);
    });

    let err = chain::produce_block(1).expect_err("produce_block should fail");
    assert_eq!(err, ChainError::NoExecutableTx);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Dropped);
    assert_eq!(loc.drop_code, DROP_CODE_MISSING);
}

#[test]
fn produce_block_marks_caller_missing() {
    init_stable_state();

    let tx_bytes = build_ic_tx_bytes_with_fee(TEST_MAX_FEE_PER_GAS, TEST_MAX_PRIORITY_FEE_PER_GAS);
    let tx_id = TxId([0x33u8; 32]);
    let envelope = StoredTxBytes::new_with_fees(
        tx_id,
        TxKind::IcSynthetic,
        tx_bytes,
        None,
        Vec::new(),
        Vec::new(),
        2_000_000_000,
        1_000_000_000,
        true,
    );
    let sender = [0x44u8; 20];
    let pending_key = SenderNonceKey::new(sender, 0);
    with_state_mut(|state| {
        state.tx_store.insert(tx_id, envelope);
        state.tx_locs.insert(tx_id, TxLoc::queued(0));
        state.pending_by_sender_nonce.insert(pending_key, tx_id);
        state.pending_meta_by_tx_id.insert(tx_id, pending_key);
        state.pending_min_nonce.insert(SenderKey::new(sender), 0);
        let key = ReadyKey::new(1, 0, 0, tx_id.0);
        state.ready_queue.insert(key, tx_id);
        state.ready_key_by_tx_id.insert(tx_id, key);
    });

    let err = chain::produce_block(1).expect_err("produce_block should fail");
    assert_eq!(err, ChainError::NoExecutableTx);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Dropped);
    assert_eq!(loc.drop_code, DROP_CODE_CALLER_MISSING);
}

#[test]
fn produce_block_marks_exec_drop() {
    init_stable_state();

    let tx_bytes = build_ic_tx_bytes_with_fee(TEST_MAX_FEE_PER_GAS, TEST_MAX_PRIORITY_FEE_PER_GAS);
    let tx_id = chain::submit_ic_tx(vec![0x11], vec![0x22], tx_bytes).expect("submit");
    let err = chain::produce_block(1).expect_err("precheck should drop and not produce");
    assert_eq!(err, ChainError::NoExecutableTx);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Dropped);
    assert_eq!(loc.drop_code, DROP_CODE_EXEC_PRECHECK);
}

#[test]
fn produce_block_marks_precheck_drop_without_nonce_bump() {
    init_stable_state();

    let tx_id = chain::submit_ic_tx(
        vec![0x99],
        vec![0xaa],
        build_ic_tx_bytes_with_custom_gas(
            50_000,
            TEST_MAX_FEE_PER_GAS,
            TEST_MAX_PRIORITY_FEE_PER_GAS,
        ),
    )
    .expect("submit");

    let sender = with_state_mut(|state| {
        let pending_key = state
            .pending_meta_by_tx_id
            .get(&tx_id)
            .expect("pending key must exist");
        let sender = pending_key.sender;
        // Force REVM precheck failure by moving account nonce ahead of pending tx nonce.
        let account_key = make_account_key(sender.0);
        let value = state
            .accounts
            .get(&account_key)
            .map(|current| {
                AccountVal::from_parts(
                    pending_key.nonce.saturating_add(1),
                    current.balance(),
                    current.code_hash(),
                )
            })
            .unwrap_or_else(|| {
                AccountVal::from_parts(pending_key.nonce.saturating_add(1), [0u8; 32], [0u8; 32])
            });
        state.accounts.insert(account_key, value);
        sender.0
    });

    let nonce_before = chain::expected_nonce_for_sender_view(sender);
    let err = chain::produce_block(1).expect_err("precheck failure should drop tx");
    assert_eq!(err, ChainError::NoExecutableTx);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Dropped);
    assert_eq!(loc.drop_code, DROP_CODE_EXEC_PRECHECK);
    let nonce_after = chain::expected_nonce_for_sender_view(sender);
    assert_eq!(nonce_after, nonce_before);
}

#[test]
fn produce_block_drop_only_purges_queue() {
    init_stable_state();

    let tx_id = TxId([0x55u8; 32]);
    let sender = [0x66u8; 20];
    let pending_key = SenderNonceKey::new(sender, 0);
    with_state_mut(|state| {
        state.tx_locs.insert(tx_id, TxLoc::queued(0));
        state.pending_by_sender_nonce.insert(pending_key, tx_id);
        state.pending_meta_by_tx_id.insert(tx_id, pending_key);
        state.pending_min_nonce.insert(SenderKey::new(sender), 0);
        let key = ReadyKey::new(1, 0, 0, tx_id.0);
        state.ready_queue.insert(key, tx_id);
        state.ready_key_by_tx_id.insert(tx_id, key);
    });

    let err = chain::produce_block(1).expect_err("produce_block should fail");
    assert_eq!(err, ChainError::NoExecutableTx);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Dropped);
    assert_eq!(loc.drop_code, DROP_CODE_MISSING);

    with_state(|state| {
        assert!(state.tx_store.get(&tx_id).is_none());
        assert!(state.ready_key_by_tx_id.get(&tx_id).is_none());
        assert_eq!(state.ready_queue.len(), 0);
        assert!(state.pending_by_sender_nonce.get(&pending_key).is_none());
    });
}

#[test]
fn produce_block_marks_block_gas_exceeded_drop() {
    init_stable_state();
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.block_gas_limit = 10;
        state.chain_state.set(chain_state);
    });

    let tx_id = chain::submit_ic_tx(
        vec![0x77],
        vec![0x88],
        build_ic_tx_bytes_with_custom_gas(
            50_000,
            TEST_MAX_FEE_PER_GAS,
            TEST_MAX_PRIORITY_FEE_PER_GAS,
        ),
    )
    .expect("submit");

    let err = chain::produce_block(1).expect_err("should drop oversized tx");
    assert_eq!(err, ChainError::NoExecutableTx);
    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Dropped);
    assert_eq!(loc.drop_code, DROP_CODE_BLOCK_GAS_EXCEEDED);
}

fn build_ic_tx_bytes_with_fee(max_fee: u128, max_priority: u128) -> Vec<u8> {
    build_ic_tx_bytes_with_custom_gas(50_000, max_fee, max_priority)
}

fn build_ic_tx_bytes_with_custom_gas(gas_limit: u64, max_fee: u128, max_priority: u128) -> Vec<u8> {
    let to = [0u8; 20];
    let value = [0u8; 32];
    let gas_limit = gas_limit.to_be_bytes();
    let nonce = 0u64.to_be_bytes();
    let max_fee = max_fee.to_be_bytes();
    let max_priority = max_priority.to_be_bytes();
    let data: Vec<u8> = Vec::new();
    let data_len = u32::try_from(data.len()).unwrap_or(0).to_be_bytes();
    let mut out = Vec::new();
    out.push(2u8);
    out.extend_from_slice(&to);
    out.extend_from_slice(&value);
    out.extend_from_slice(&gas_limit);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&max_fee);
    out.extend_from_slice(&max_priority);
    out.extend_from_slice(&data_len);
    out.extend_from_slice(&data);
    out
}
