//! どこで: Phase0テスト / 何を: StableStateの初期化とupgrade相当再接続 / なぜ: 結線の健全性確認

use evm_db::chain_data::{
    Head, PruneStateV1, ReceiptLike, SenderKey, TxId, TxIndexEntry, TxLoc,
};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
use evm_db::types::keys::make_account_key;
use evm_db::types::values::AccountVal;
use ic_stable_structures::Storable;
use verified_core::upgrade_safety::upgrade_core_observation_preserved_raw;

#[test]
fn stable_state_init_and_insert() {
    init_stable_state();
    let addr = [0x77u8; 20];
    let key = make_account_key(addr);
    let val = AccountVal([0x88u8; 72]);

    with_state_mut(|state| {
        state.accounts.insert(key, val);
        let found = state.accounts.get(&key);
        assert_eq!(found, Some(val));
    });
}

#[test]
fn stable_state_reinit_preserves_core_upgrade_observations() {
    init_stable_state();

    let tx_id = TxId([0x91; 32]);
    let sender = SenderKey::new([0x31; 20]);
    let head = Head {
        number: 42,
        block_hash: [0x42; 32],
        timestamp: 123,
    };
    let mut prune_state = PruneStateV1::new();
    prune_state.set_pruned_before(12);
    prune_state.next_prune_block = 13;
    let tx_index = TxIndexEntry {
        block_number: 42,
        tx_index: 0,
    };
    let receipt = ReceiptLike {
        tx_id,
        block_number: 42,
        tx_index: 0,
        status: 1,
        gas_used: 21_000,
        effective_gas_price: 1,
        l1_data_fee: 0,
        operator_fee: 0,
        total_fee: 21_000,
        return_data_hash: [0x55; 32],
        return_data: Vec::new(),
        contract_address: None,
        logs: Vec::new(),
    };
    let tx_loc = TxLoc::included(42, 0);

    with_state_mut(|state| {
        state.head.set(head);
        state.prune_state.set(prune_state);
        state.pending_current_by_sender.insert(sender, tx_id);
        let tx_index_ptr = state.blob_store.store_bytes(&tx_index.to_bytes()).unwrap();
        let receipt_ptr = state.blob_store.store_bytes(&receipt.to_bytes()).unwrap();
        state.tx_index.insert(tx_id, tx_index_ptr);
        state.receipts.insert(tx_id, receipt_ptr);
        state.tx_locs.insert(tx_id, tx_loc);
    });

    init_stable_state();

    with_state(|state| {
        let restored_tx_index_ptr = state.tx_index.get(&tx_id).expect("tx index pointer");
        let restored_tx_index =
            TxIndexEntry::from_bytes(state.blob_store.read(&restored_tx_index_ptr).unwrap().into());
        let restored_receipt_ptr = state.receipts.get(&tx_id).expect("receipt pointer");
        let restored_receipt =
            ReceiptLike::from_bytes(state.blob_store.read(&restored_receipt_ptr).unwrap().into());
        let restored_loc = state.tx_locs.get(&tx_id).expect("tx loc");

        let preserved = upgrade_core_observation_preserved_raw(
            u64::from(*state.head.get() == head),
            u64::from(*state.prune_state.get() == prune_state),
            u64::from(state.pending_current_by_sender.get(&sender) == Some(tx_id)),
            u64::from(restored_receipt == receipt),
            u64::from(restored_tx_index == tx_index),
            u64::from(restored_loc == tx_loc),
        );
        assert!(preserved);
    });
}
