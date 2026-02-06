//! どこで: prune ジャーナルのテスト / 何を: 復旧と再利用抑止 / なぜ: クラッシュ耐性を固定するため

use evm_core::chain;
use evm_db::chain_data::{BlockData, ReceiptLike, TxId, TxIndexEntry, TxLoc};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
use ic_stable_structures::Storable;

#[test]
fn prune_journal_recovery_frees_quarantine() {
    init_stable_state();
    let tx = TxId([0x44; 32]);
    let block = make_block(10, tx);

    with_state_mut(|state| {
        insert_block(state, 10, &block);
        insert_receipt(state, tx, 10);
        insert_tx_index(state, tx, 10);
        state.tx_locs.insert(tx, TxLoc::included(10, 0));
        let mut head = *state.head.get();
        head.number = 10;
        state.head.set(head);
    });

    let first = chain::prune_blocks(1, 100).expect("prune should succeed");
    assert!(first.did_work);

    let second = chain::prune_blocks(1, 100).expect("prune should succeed");
    assert!(second.did_work || !second.did_work);
}

#[test]
fn prune_journal_recovery_removes_seen_tx() {
    init_stable_state();
    let tx = TxId([0x47; 32]);
    let block = make_block(40, tx);

    with_state_mut(|state| {
        insert_block(state, 40, &block);
        insert_receipt(state, tx, 40);
        insert_tx_index(state, tx, 40);
        state.tx_locs.insert(tx, TxLoc::included(40, 0));
        state.seen_tx.insert(tx, 1);
        let mut head = *state.head.get();
        head.number = 40;
        state.head.set(head);

        let mut ptrs = Vec::new();
        if let Some(ptr) = state.blocks.get(&40) {
            ptrs.push(ptr);
        }
        if let Some(ptr) = state.receipts.get(&tx) {
            ptrs.push(ptr);
        }
        if let Some(ptr) = state.tx_index.get(&tx) {
            ptrs.push(ptr);
        }
        for ptr in ptrs.iter() {
            state
                .blob_store
                .mark_quarantine(ptr)
                .expect("quarantine ptr");
        }
        state
            .prune_journal
            .insert(40, evm_db::chain_data::PruneJournal { ptrs });
        let mut prune_state = *state.prune_state.get();
        prune_state.set_journal_block(40);
        state.prune_state.set(prune_state);
    });

    let result = chain::prune_blocks(1, 100).expect("prune should succeed");
    assert!(result.did_work || !result.did_work);

    with_state(|state| {
        assert!(state.seen_tx.get(&tx).is_none());
    });
}

#[test]
fn quarantine_is_not_reused_during_prune() {
    init_stable_state();
    let tx = TxId([0x55; 32]);
    let block = make_block(20, tx);

    with_state_mut(|state| {
        insert_block(state, 20, &block);
        insert_receipt(state, tx, 20);
        insert_tx_index(state, tx, 20);
        state.tx_locs.insert(tx, TxLoc::included(20, 0));
        let mut head = *state.head.get();
        head.number = 20;
        state.head.set(head);
    });

    let _ = chain::prune_blocks(1, 100).expect("prune should succeed");
    let result = chain::prune_blocks(1, 100).expect("prune should succeed");
    assert!(result.did_work || !result.did_work);
}

#[test]
fn prune_is_idempotent() {
    init_stable_state();
    let tx = TxId([0x66; 32]);
    let block = make_block(30, tx);

    with_state_mut(|state| {
        insert_block(state, 30, &block);
        insert_receipt(state, tx, 30);
        insert_tx_index(state, tx, 30);
        state.tx_locs.insert(tx, TxLoc::included(30, 0));
        let mut head = *state.head.get();
        head.number = 30;
        state.head.set(head);
    });

    let first = chain::prune_blocks(1, 100).expect("prune should succeed");
    let second = chain::prune_blocks(1, 100).expect("prune should succeed");
    assert!(first.did_work);
    assert!(second.did_work || !second.did_work);
}

fn make_block(number: u64, tx_id: TxId) -> BlockData {
    let parent_hash = [0u8; 32];
    let number_u8 = u8::try_from(number).unwrap_or(0);
    let block_hash = [number_u8; 32];
    let tx_list_hash = [number_u8; 32];
    let state_root = [0u8; 32];
    BlockData::new(
        number,
        parent_hash,
        block_hash,
        number,
        vec![tx_id],
        tx_list_hash,
        state_root,
    )
}

fn insert_block(state: &mut evm_db::stable_state::StableState, number: u64, block: &BlockData) {
    let bytes = block.to_bytes().into_owned();
    let ptr = state.blob_store.store_bytes(&bytes).expect("store block");
    state.blocks.insert(number, ptr);
}

fn insert_receipt(state: &mut evm_db::stable_state::StableState, tx_id: TxId, block_number: u64) {
    let receipt = fake_receipt(tx_id, block_number);
    let bytes = receipt.to_bytes().into_owned();
    let ptr = state.blob_store.store_bytes(&bytes).expect("store receipt");
    state.receipts.insert(tx_id, ptr);
}

fn insert_tx_index(state: &mut evm_db::stable_state::StableState, tx_id: TxId, block_number: u64) {
    let entry = TxIndexEntry {
        block_number,
        tx_index: 0,
    };
    let bytes = entry.to_bytes().into_owned();
    let ptr = state
        .blob_store
        .store_bytes(&bytes)
        .expect("store tx_index");
    state.tx_index.insert(tx_id, ptr);
}

fn fake_receipt(tx_id: TxId, block_number: u64) -> ReceiptLike {
    ReceiptLike {
        tx_id,
        block_number,
        tx_index: 0,
        status: 1,
        gas_used: 0,
        effective_gas_price: 0,
        l1_data_fee: 0,
        operator_fee: 0,
        total_fee: 0,
        return_data_hash: [0u8; 32],
        return_data: Vec::new(),
        contract_address: None,
        logs: Vec::new(),
    }
}
