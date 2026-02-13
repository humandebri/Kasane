//! どこで: export API のテスト / 何を: cursor整合・max_bytes・Pruned / なぜ: 仕様の逸脱を防ぐため

use evm_core::export::{export_blocks, ExportCursor, ExportError};
use evm_db::chain_data::{BlockData, ReceiptLike, TxId, TxIndexEntry, TxLoc};
use evm_db::stable_state::{init_stable_state, with_state_mut};
use evm_db::Storable;

#[test]
fn export_cursor_alignment_and_next_cursor() {
    init_stable_state();
    let tx = TxId([0x11; 32]);
    let block = make_block(1, tx);

    with_state_mut(|state| {
        insert_block(state, 1, &block);
        insert_receipt(state, tx, 1);
        insert_tx_index(state, tx, 1);
        state.tx_locs.insert(tx, TxLoc::included(1, 0));
        let mut head = *state.head.get();
        head.number = 1;
        state.head.set(head);
    });

    let cursor = ExportCursor {
        block_number: 1,
        segment: 0,
        byte_offset: 1,
    };
    let result = export_blocks(Some(cursor), 10).expect("export should succeed");
    assert!(!result.chunks.is_empty());
    let first = &result.chunks[0];
    assert_eq!(first.segment, 0);
    assert_eq!(first.start, 1);
    let taken = u32::try_from(first.bytes.len()).unwrap_or(0);
    let next = result.next_cursor.expect("next cursor");
    assert_eq!(next.block_number, 1);
    assert_eq!(next.segment, 0);
    assert_eq!(next.byte_offset, first.start.saturating_add(taken));
}

#[test]
fn export_respects_max_bytes() {
    init_stable_state();
    let tx = TxId([0x22; 32]);
    let block = make_block(2, tx);
    with_state_mut(|state| {
        insert_block(state, 2, &block);
        insert_receipt(state, tx, 2);
        insert_tx_index(state, tx, 2);
        state.tx_locs.insert(tx, TxLoc::included(2, 0));
        let mut head = *state.head.get();
        head.number = 2;
        state.head.set(head);
    });

    let cursor = ExportCursor {
        block_number: 2,
        segment: 0,
        byte_offset: 0,
    };
    let result = export_blocks(Some(cursor), 1).expect("export should succeed");
    let total: usize = result.chunks.iter().map(|c| c.bytes.len()).sum();
    assert!(total <= 1);
}

#[test]
fn export_pruned_and_oldest_exportable() {
    init_stable_state();
    let tx = TxId([0x33; 32]);
    let block = make_block(6, tx);
    with_state_mut(|state| {
        insert_block(state, 6, &block);
        insert_receipt(state, tx, 6);
        insert_tx_index(state, tx, 6);
        state.tx_locs.insert(tx, TxLoc::included(6, 0));
        let mut head = *state.head.get();
        head.number = 6;
        state.head.set(head);

        let mut prune_state = *state.prune_state.get();
        prune_state.set_pruned_before(5);
        state.prune_state.set(prune_state);
    });

    let cursor = ExportCursor {
        block_number: 5,
        segment: 0,
        byte_offset: 0,
    };
    let err = export_blocks(Some(cursor), 10).expect_err("should be pruned");
    assert!(matches!(err, ExportError::Pruned { .. }));

    let ok = export_blocks(None, 10).expect("export should succeed");
    assert!(!ok.chunks.is_empty());
    let next = ok.next_cursor.expect("next cursor");
    assert_eq!(next.block_number, 6);
}

#[test]
fn export_advances_across_blocks_when_budget_allows() {
    init_stable_state();
    let tx1 = TxId([0x44; 32]);
    let tx2 = TxId([0x55; 32]);
    let block1 = make_block(1, tx1);
    let block2 = make_block(2, tx2);
    with_state_mut(|state| {
        insert_block(state, 1, &block1);
        insert_receipt(state, tx1, 1);
        insert_tx_index(state, tx1, 1);
        state.tx_locs.insert(tx1, TxLoc::included(1, 0));

        insert_block(state, 2, &block2);
        insert_receipt(state, tx2, 2);
        insert_tx_index(state, tx2, 2);
        state.tx_locs.insert(tx2, TxLoc::included(2, 0));

        let mut head = *state.head.get();
        head.number = 2;
        state.head.set(head);
    });

    let cursor = ExportCursor {
        block_number: 1,
        segment: 0,
        byte_offset: 0,
    };
    let result = export_blocks(Some(cursor), 1_000_000).expect("export should succeed");
    assert!(!result.chunks.is_empty());
    let next = result.next_cursor.expect("next cursor");
    assert_eq!(next.block_number, 3);
}

#[test]
fn export_rejects_segment_out_of_range() {
    init_stable_state();
    let tx = TxId([0x66; 32]);
    let block = make_block(1, tx);
    with_state_mut(|state| {
        insert_block(state, 1, &block);
        insert_receipt(state, tx, 1);
        insert_tx_index(state, tx, 1);
        state.tx_locs.insert(tx, TxLoc::included(1, 0));
        let mut head = *state.head.get();
        head.number = 1;
        state.head.set(head);
    });

    let cursor = ExportCursor {
        block_number: 1,
        segment: 3,
        byte_offset: 0,
    };
    let err = export_blocks(Some(cursor), 10).expect_err("should be invalid cursor");
    assert!(matches!(err, ExportError::InvalidCursor(_)));
}

#[test]
fn export_advances_on_segment_boundary() {
    init_stable_state();
    let tx = TxId([0x77; 32]);
    let block = make_block(1, tx);
    with_state_mut(|state| {
        insert_block(state, 1, &block);
        insert_receipt(state, tx, 1);
        insert_tx_index(state, tx, 1);
        state.tx_locs.insert(tx, TxLoc::included(1, 0));
        let mut head = *state.head.get();
        head.number = 1;
        state.head.set(head);
    });

    let block_bytes = block.to_bytes().into_owned();
    let cursor = ExportCursor {
        block_number: 1,
        segment: 0,
        byte_offset: u32::try_from(block_bytes.len()).unwrap_or(0),
    };
    let result = export_blocks(Some(cursor), 1).expect("export should succeed");
    assert!(!result.chunks.is_empty());
    let next = result.next_cursor.expect("next cursor");
    assert_eq!(next.block_number, 1);
    assert_eq!(next.segment, 1);
    assert_eq!(next.byte_offset, 1);
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
        21_000,
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
