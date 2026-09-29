//! どこで: Phase1 pruning テスト / 何を: prune_blocks の削除と状態更新 / なぜ: None判定の前提を保証するため

use evm_core::{chain, hash};
use evm_db::chain_data::{
    BlockData, PrunedMarkerBlockKey, ReceiptLike, StoredTxBytes, TxId, TxIndexEntry, TxKind, TxLoc,
};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
use evm_db::Storable;

#[test]
fn prune_blocks_removes_old_data() {
    init_stable_state();

    let raw1 = vec![0x02, 0x11];
    let eth_hash1 = TxId(hash::keccak256(&raw1));
    let tx1 = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw1,
        None,
        None,
        None,
    ));
    let raw2 = vec![0x02, 0x22];
    let eth_hash2 = TxId(hash::keccak256(&raw2));
    let tx2 = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw2,
        None,
        None,
        None,
    ));
    let tx3 = TxId([0x33; 32]);

    let block1 = make_block(1, tx1);
    let block2 = make_block(2, tx2);
    let block3 = make_block(3, tx3);

    with_state_mut(|state| {
        insert_block(state, 1, &block1);
        insert_block(state, 2, &block2);
        insert_block(state, 3, &block3);
        insert_tx_index(state, tx1, 1);
        insert_tx_index(state, tx2, 2);
        insert_tx_index(state, tx3, 3);
        insert_receipt(state, tx1, 1);
        insert_receipt(state, tx2, 2);
        insert_receipt(state, tx3, 3);
        state.tx_store.insert(
            tx1,
            StoredTxBytes::new_with_fees(
                tx1,
                TxKind::EthSigned,
                raw1.clone(),
                None,
                Vec::new(),
                Vec::new(),
                0,
                0,
                false,
            ),
        );
        state.tx_store.insert(
            tx2,
            StoredTxBytes::new_with_fees(
                tx2,
                TxKind::EthSigned,
                raw2.clone(),
                None,
                Vec::new(),
                Vec::new(),
                0,
                0,
                false,
            ),
        );
        state.eth_tx_hash_index.insert(eth_hash1, tx1);
        state.eth_tx_hash_index.insert(eth_hash2, tx2);
        state.seen_tx.insert(tx1, 1);
        state.seen_tx.insert(tx2, 1);
        state.seen_tx.insert(tx3, 1);
        state.tx_locs.insert(tx1, TxLoc::included(1, 0));
        state.tx_locs.insert(tx2, TxLoc::included(2, 0));
        state.tx_locs.insert(tx3, TxLoc::included(3, 0));
        let mut head = *state.head.get();
        head.number = 3;
        state.head.set(head);
    });

    let result = chain::prune_blocks(1, 100).expect("prune should succeed");
    assert!(result.did_work);
    assert_eq!(result.pruned_before_block, Some(2));

    with_state(|state| {
        assert!(state.blocks.get(&1).is_none());
        assert!(state.blocks.get(&2).is_none());
        assert!(state.blocks.get(&3).is_some());
        assert!(state.receipts.get(&tx1).is_none());
        assert!(state.receipts.get(&tx2).is_none());
        assert!(state.receipts.get(&tx3).is_some());
        assert!(state.tx_index.get(&tx1).is_none());
        assert!(state.tx_index.get(&tx2).is_none());
        assert!(state.tx_index.get(&tx3).is_some());
        assert!(state.tx_locs.get(&tx1).is_none());
        assert!(state.tx_locs.get(&tx2).is_none());
        assert!(state.tx_locs.get(&tx3).is_some());
        assert!(state.pruned_tx_locs.get(&tx1).is_none());
        assert_eq!(state.pruned_tx_locs.get(&tx2), Some(TxLoc::included(2, 0)));
        assert!(state.pruned_tx_locs.get(&tx3).is_none());
        assert!(state.eth_tx_hash_index.get(&eth_hash1).is_none());
        assert!(state.eth_tx_hash_index.get(&eth_hash2).is_none());
        assert!(state.pruned_eth_tx_hash_index.get(&eth_hash1).is_none());
        assert_eq!(state.pruned_eth_tx_hash_index.get(&eth_hash2), Some(tx2));
        assert!(state.pruned_marker_eth_hash_by_tx_id.get(&tx1).is_none());
        assert_eq!(
            state.pruned_marker_eth_hash_by_tx_id.get(&tx2),
            Some(eth_hash2)
        );
        assert!(state
            .pruned_marker_block_index
            .get(&PrunedMarkerBlockKey::new(1, tx1.0))
            .is_none());
        assert_eq!(
            state
                .pruned_marker_block_index
                .get(&PrunedMarkerBlockKey::new(2, tx2.0)),
            Some(tx2)
        );
        assert!(state.seen_tx.get(&tx1).is_none());
        assert!(state.seen_tx.get(&tx2).is_none());
        assert!(state.seen_tx.get(&tx3).is_some());
    });
}

#[test]
fn prune_blocks_keeps_head_and_retain_range() {
    init_stable_state();

    let txs = [
        TxId([0x11; 32]),
        TxId([0x22; 32]),
        TxId([0x33; 32]),
        TxId([0x44; 32]),
        TxId([0x55; 32]),
    ];

    with_state_mut(|state| {
        for (idx, tx) in txs.iter().copied().enumerate() {
            let block_number = u64::try_from(idx + 1).expect("block number");
            let block = make_block(block_number, tx);
            insert_block(state, block_number, &block);
            insert_tx_index(state, tx, block_number);
            insert_receipt(state, tx, block_number);
            state.seen_tx.insert(tx, 1);
            state.tx_locs.insert(tx, TxLoc::included(block_number, 0));
        }
        let mut head = *state.head.get();
        head.number = 5;
        state.head.set(head);
    });

    let result = chain::prune_blocks(2, 100).expect("prune should succeed");
    assert!(result.did_work);
    assert_eq!(result.pruned_before_block, Some(3));

    with_state(|state| {
        for tx in txs.iter().take(3) {
            assert!(state.receipts.get(tx).is_none());
            assert!(state.tx_index.get(tx).is_none());
            assert!(state.tx_locs.get(tx).is_none());
            assert!(state.seen_tx.get(tx).is_none());
        }
        for (block_number, tx) in [(4, txs[3]), (5, txs[4])] {
            assert!(state.blocks.get(&block_number).is_some());
            assert!(state.receipts.get(&tx).is_some());
            assert!(state.tx_index.get(&tx).is_some());
            assert!(state.tx_locs.get(&tx).is_some());
            assert!(state.seen_tx.get(&tx).is_some());
        }
        assert_eq!(state.head.get().number, 5);
    });
}

#[test]
fn prune_blocks_respects_max_ops() {
    init_stable_state();

    let tx1 = TxId([0x11; 32]);
    let tx2 = TxId([0x22; 32]);
    let tx3 = TxId([0x33; 32]);

    let block1 = make_block(1, tx1);
    let block2 = make_block(2, tx2);
    let block3 = make_block(3, tx3);

    with_state_mut(|state| {
        insert_block(state, 1, &block1);
        insert_block(state, 2, &block2);
        insert_block(state, 3, &block3);
        insert_tx_index(state, tx1, 1);
        insert_tx_index(state, tx2, 2);
        insert_tx_index(state, tx3, 3);
        insert_receipt(state, tx1, 1);
        insert_receipt(state, tx2, 2);
        insert_receipt(state, tx3, 3);
        state.seen_tx.insert(tx1, 1);
        state.seen_tx.insert(tx2, 1);
        state.seen_tx.insert(tx3, 1);
        state.tx_locs.insert(tx1, TxLoc::included(1, 0));
        state.tx_locs.insert(tx2, TxLoc::included(2, 0));
        state.tx_locs.insert(tx3, TxLoc::included(3, 0));
        let mut head = *state.head.get();
        head.number = 3;
        state.head.set(head);
    });

    let result = chain::prune_blocks(1, 6).expect("prune should succeed");
    assert!(result.did_work);
    assert_eq!(result.pruned_before_block, Some(1));
    assert_eq!(result.remaining, 1);

    with_state(|state| {
        assert!(state.blocks.get(&1).is_none());
        assert!(state.blocks.get(&2).is_some());
        assert!(state.blocks.get(&3).is_some());
    });
}

#[test]
fn prune_blocks_resumes_after_max_ops_stop() {
    init_stable_state();

    let tx1 = TxId([0x41; 32]);
    let tx2 = TxId([0x42; 32]);
    let tx3 = TxId([0x43; 32]);

    let block1 = make_block(1, tx1);
    let block2 = make_block(2, tx2);
    let block3 = make_block(3, tx3);

    with_state_mut(|state| {
        insert_block(state, 1, &block1);
        insert_block(state, 2, &block2);
        insert_block(state, 3, &block3);
        insert_tx_index(state, tx1, 1);
        insert_tx_index(state, tx2, 2);
        insert_tx_index(state, tx3, 3);
        insert_receipt(state, tx1, 1);
        insert_receipt(state, tx2, 2);
        insert_receipt(state, tx3, 3);
        state.seen_tx.insert(tx1, 1);
        state.seen_tx.insert(tx2, 1);
        state.seen_tx.insert(tx3, 1);
        state.tx_locs.insert(tx1, TxLoc::included(1, 0));
        state.tx_locs.insert(tx2, TxLoc::included(2, 0));
        state.tx_locs.insert(tx3, TxLoc::included(3, 0));
        let mut head = *state.head.get();
        head.number = 3;
        state.head.set(head);
    });

    let first = chain::prune_blocks(1, 6).expect("first partial prune should succeed");
    assert!(first.did_work);
    assert_eq!(first.pruned_before_block, Some(1));
    assert_eq!(first.remaining, 1);

    let second = chain::prune_blocks(1, 6).expect("second partial prune should resume");
    assert!(second.did_work);
    assert_eq!(second.pruned_before_block, Some(2));
    assert_eq!(second.remaining, 1);

    let third = chain::prune_blocks(1, 3).expect("marker cleanup should finish");
    assert!(third.did_work);
    assert_eq!(third.pruned_before_block, Some(2));
    assert_eq!(third.remaining, 0);

    with_state(|state| {
        assert!(state.blocks.get(&1).is_none());
        assert!(state.blocks.get(&2).is_none());
        assert!(state.blocks.get(&3).is_some());
        assert!(state.pruned_tx_locs.get(&tx1).is_none());
        assert_eq!(state.pruned_tx_locs.get(&tx2), Some(TxLoc::included(2, 0)));
        assert_eq!(state.prune_state.get().pruned_before(), Some(2));
        assert!(state.prune_state.get().journal_block().is_none());
    });
}

#[test]
fn prune_blocks_cleans_old_pruned_markers_without_block_work() {
    init_stable_state();

    let tx1 = TxId([0x61; 32]);
    let tx2 = TxId([0x62; 32]);
    let eth_hash1 = TxId([0xe1; 32]);
    let eth_hash2 = TxId([0xe2; 32]);

    with_state_mut(|state| {
        insert_pruned_marker(state, 1, tx1, eth_hash1);
        insert_pruned_marker(state, 2, tx2, eth_hash2);
        let mut prune_state = *state.prune_state.get();
        prune_state.set_pruned_before(2);
        state.prune_state.set(prune_state);
        let mut head = *state.head.get();
        head.number = 3;
        state.head.set(head);
    });

    let result = chain::prune_blocks(1, 10).expect("marker cleanup should succeed");
    assert!(result.did_work);
    assert_eq!(result.remaining, 0);
    assert_eq!(result.pruned_before_block, Some(2));

    with_state(|state| {
        assert!(state.pruned_tx_locs.get(&tx1).is_none());
        assert_eq!(state.pruned_tx_locs.get(&tx2), Some(TxLoc::included(2, 0)));
        assert!(state.pruned_eth_tx_hash_index.get(&eth_hash1).is_none());
        assert_eq!(state.pruned_eth_tx_hash_index.get(&eth_hash2), Some(tx2));
        assert!(state.pruned_marker_eth_hash_by_tx_id.get(&tx1).is_none());
        assert_eq!(
            state.pruned_marker_eth_hash_by_tx_id.get(&tx2),
            Some(eth_hash2)
        );
        assert!(state
            .pruned_marker_block_index
            .get(&PrunedMarkerBlockKey::new(1, tx1.0))
            .is_none());
        assert_eq!(
            state
                .pruned_marker_block_index
                .get(&PrunedMarkerBlockKey::new(2, tx2.0)),
            Some(tx2)
        );
    });
}

#[test]
fn prune_blocks_reports_remaining_marker_cleanup_when_budget_limited() {
    init_stable_state();

    let tx1 = TxId([0x71; 32]);
    let tx2 = TxId([0x72; 32]);
    let eth_hash1 = TxId([0xf1; 32]);
    let eth_hash2 = TxId([0xf2; 32]);

    with_state_mut(|state| {
        insert_pruned_marker(state, 1, tx1, eth_hash1);
        insert_pruned_marker(state, 2, tx2, eth_hash2);
        let mut prune_state = *state.prune_state.get();
        prune_state.set_pruned_before(3);
        state.prune_state.set(prune_state);
        let mut head = *state.head.get();
        head.number = 4;
        state.head.set(head);
    });

    let first = chain::prune_blocks(1, 4).expect("first marker cleanup should succeed");
    assert!(first.did_work);
    assert_eq!(first.remaining, 1);
    assert_eq!(first.pruned_before_block, Some(3));
    with_state(|state| {
        assert!(state.pruned_tx_locs.get(&tx1).is_none());
        assert_eq!(state.pruned_tx_locs.get(&tx2), Some(TxLoc::included(2, 0)));
    });

    let second = chain::prune_blocks(1, 4).expect("second marker cleanup should succeed");
    assert!(second.did_work);
    assert_eq!(second.remaining, 0);
    assert_eq!(second.pruned_before_block, Some(3));
    with_state(|state| {
        assert!(state.pruned_tx_locs.get(&tx1).is_none());
        assert!(state.pruned_tx_locs.get(&tx2).is_none());
        assert!(state.pruned_eth_tx_hash_index.get(&eth_hash1).is_none());
        assert!(state.pruned_eth_tx_hash_index.get(&eth_hash2).is_none());
        assert!(state.pruned_marker_eth_hash_by_tx_id.get(&tx1).is_none());
        assert!(state.pruned_marker_eth_hash_by_tx_id.get(&tx2).is_none());
    });
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
        1_000_000_000,
        3_000_000,
        0,
        [0u8; 20],
        vec![tx_id],
        tx_list_hash,
        state_root,
    )
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

fn insert_pruned_marker(
    state: &mut evm_db::stable_state::StableState,
    block_number: u64,
    tx_id: TxId,
    eth_hash: TxId,
) {
    state
        .pruned_tx_locs
        .insert(tx_id, TxLoc::included(block_number, 0));
    state
        .pruned_marker_block_index
        .insert(PrunedMarkerBlockKey::new(block_number, tx_id.0), tx_id);
    state.pruned_eth_tx_hash_index.insert(eth_hash, tx_id);
    state
        .pruned_marker_eth_hash_by_tx_id
        .insert(tx_id, eth_hash);
}
