//! どこで: Phase1テスト / 何を: produce_block の drop_code / なぜ: 失敗理由の可視化を固定するため

use evm_core::chain::{self, ChainError};
use evm_db::chain_data::constants::{
    DROP_CODE_CALLER_MISSING, DROP_CODE_DECODE, DROP_CODE_MISSING,
};
use evm_db::chain_data::{TxEnvelope, TxId, TxKind, TxLocKind};
use evm_db::stable_state::{init_stable_state, with_state_mut};

#[test]
fn produce_block_marks_decode_drop() {
    init_stable_state();

    let bad_tx = vec![0x01];
    let tx_id = chain::submit_tx(evm_db::chain_data::TxKind::EthSigned, bad_tx)
        .expect("submit");

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
    with_state_mut(|state| {
        let mut meta = *state.queue_meta.get();
        let seq = meta.push();
        state.queue_meta.set(meta);
        state.queue.insert(seq, tx_id);
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

    let tx_bytes = build_ic_tx_bytes();
    let tx_id = TxId([0x33u8; 32]);
    let envelope = TxEnvelope::new(tx_id, TxKind::IcSynthetic, tx_bytes);
    with_state_mut(|state| {
        state.tx_store.insert(tx_id, envelope);
        let mut meta = *state.queue_meta.get();
        let seq = meta.push();
        state.queue_meta.set(meta);
        state.queue.insert(seq, tx_id);
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

    let tx_bytes = build_ic_tx_bytes();
    let tx_id = TxId([0x44u8; 32]);
    let envelope = TxEnvelope::new_with_caller(tx_id, TxKind::IcSynthetic, tx_bytes, [0x11u8; 20]);
    with_state_mut(|state| {
        state.tx_store.insert(tx_id, envelope);
        let mut meta = *state.queue_meta.get();
        let seq = meta.push();
        state.queue_meta.set(meta);
        state.queue.insert(seq, tx_id);
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1;
        state.chain_state.set(chain_state);
    });

    let block = chain::produce_block(1).expect("produce_block should succeed");
    assert_eq!(block.tx_ids.len(), 1);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Included);

    let receipt = chain::get_receipt(&tx_id).expect("receipt");
    assert_eq!(receipt.status, 0);
}

fn build_ic_tx_bytes() -> Vec<u8> {
    let version = 1u8;
    let to = [0u8; 20];
    let value = [0u8; 32];
    let gas_limit = 50_000u64.to_be_bytes();
    let nonce = 0u64.to_be_bytes();
    let data: Vec<u8> = Vec::new();
    let data_len = u32::try_from(data.len()).unwrap_or(0).to_be_bytes();
    let mut out = Vec::new();
    out.push(version);
    out.extend_from_slice(&to);
    out.extend_from_slice(&value);
    out.extend_from_slice(&gas_limit);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&data_len);
    out.extend_from_slice(&data);
    out
}
