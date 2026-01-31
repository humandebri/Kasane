//! どこで: Phase1テスト / 何を: produce_block の drop_code / なぜ: 失敗理由の可視化を固定するため

use evm_core::chain::{self, ChainError};
use evm_db::chain_data::constants::{
    DROP_CODE_DECODE, DROP_CODE_INVALID_FEE,
};
use evm_db::chain_data::{ReadyKey, SenderKey, SenderNonceKey, StoredTx, TxId, TxKind, TxLoc, TxLocKind};
use evm_db::stable_state::{init_stable_state, with_state_mut};

#[test]
fn produce_block_marks_decode_drop() {
    init_stable_state();

    let tx_id = TxId([0x10u8; 32]);
    let envelope = StoredTx::new_with_fees(
        tx_id,
        TxKind::EthSigned,
        vec![0x01],
        None,
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
        let key = ReadyKey::new(1, 0, tx_id.0);
        state.ready_queue.insert(key, tx_id);
        state.ready_key_by_tx_id.insert(tx_id, key);
    });

    let err = chain::produce_block(1).expect_err("produce_block should fail");
    assert_eq!(err, ChainError::NoExecutableTx);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Dropped);
    assert_eq!(loc.drop_code, DROP_CODE_DECODE);
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
        let key = ReadyKey::new(1, 0, tx_id.0);
        state.ready_queue.insert(key, tx_id);
        state.ready_key_by_tx_id.insert(tx_id, key);
    });

    let err = chain::produce_block(1).expect_err("produce_block should fail");
    assert_eq!(err, ChainError::NoExecutableTx);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Dropped);
    assert_eq!(loc.drop_code, DROP_CODE_INVALID_FEE);
}

#[test]
fn produce_block_marks_caller_missing() {
    init_stable_state();

    let tx_bytes = build_ic_tx_bytes_with_fee(2_000_000_000, 1_000_000_000);
    let tx_id = TxId([0x33u8; 32]);
    let envelope = StoredTx::new_with_fees(
        tx_id,
        TxKind::IcSynthetic,
        tx_bytes,
        None,
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
        let key = ReadyKey::new(1, 0, tx_id.0);
        state.ready_queue.insert(key, tx_id);
        state.ready_key_by_tx_id.insert(tx_id, key);
    });

    let err = chain::produce_block(1).expect_err("produce_block should fail");
    assert_eq!(err, ChainError::NoExecutableTx);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Dropped);
    assert_eq!(loc.drop_code, DROP_CODE_DECODE);
}

#[test]
fn produce_block_marks_exec_drop() {
    init_stable_state();

    let tx_bytes = build_ic_tx_bytes_with_fee(2_000_000_000, 1_000_000_000);
    let caller = [0x11u8; 20];
    let tx_id = chain::submit_ic_tx(caller, vec![0x11], vec![0x22], tx_bytes).expect("submit");
    let block = chain::produce_block(1).expect("produce_block should succeed");
    assert_eq!(block.tx_ids.len(), 1);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Included);
}

fn build_ic_tx_bytes_with_fee(max_fee: u128, max_priority: u128) -> Vec<u8> {
    let to = [0u8; 20];
    let value = [0u8; 32];
    let gas_limit = 50_000u64.to_be_bytes();
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
