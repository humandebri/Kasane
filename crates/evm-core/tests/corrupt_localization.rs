//! どこで: evm-core read helper
//! 何を: 破損レコードを局所欠損として扱うことを検証
//! なぜ: 1件の decode failure で正常データ参照まで巻き込まないため

use evm_core::chain;
use evm_db::chain_data::{BlockData, ReceiptLike, StoredTxBytes, TxId, TxLoc};
use evm_db::stable_state::{init_stable_state, with_state_mut};
use evm_db::Storable;
use std::borrow::Cow;

#[test]
fn get_block_returns_none_for_corrupt_block_payload() {
    init_stable_state();
    with_state_mut(|state| {
        let corrupt = BlockData::from_bytes(Cow::Owned(vec![0u8; 1]));
        let ptr = state
            .blob_store
            .store_bytes(corrupt.to_bytes().as_ref())
            .expect("store corrupt block");
        state.blocks.insert(7, ptr);
    });

    assert!(chain::get_block(7).is_none());
}

#[test]
fn get_receipt_returns_none_for_corrupt_receipt_payload() {
    init_stable_state();
    let tx_id = TxId([0x41u8; 32]);
    with_state_mut(|state| {
        let corrupt = ReceiptLike::from_bytes(Cow::Owned(vec![0u8; 1]));
        let ptr = state
            .blob_store
            .store_bytes(corrupt.to_bytes().as_ref())
            .expect("store corrupt receipt");
        state.receipts.insert(tx_id, ptr);
    });

    assert!(chain::get_receipt(&tx_id).is_none());
}

#[test]
fn get_tx_envelope_returns_none_for_invalid_stored_tx() {
    init_stable_state();
    let tx_id = TxId([0x42u8; 32]);
    with_state_mut(|state| {
        state
            .tx_store
            .insert(tx_id, StoredTxBytes::from_bytes(Cow::Owned(vec![0u8; 1])));
    });

    assert!(chain::get_tx_envelope(&tx_id).is_none());
}

#[test]
fn get_tx_loc_returns_none_for_decode_failure_placeholder() {
    init_stable_state();
    let tx_id = TxId([0x43u8; 32]);
    with_state_mut(|state| {
        state
            .tx_locs
            .insert(tx_id, TxLoc::decode_failure_placeholder());
    });

    assert!(chain::get_tx_loc(&tx_id).is_none());
}
