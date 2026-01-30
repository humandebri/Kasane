//! どこで: Phase1 pruning テスト / 何を: prune_blocks の削除と状態更新 / なぜ: None判定の前提を保証するため

use evm_core::chain;
use evm_db::chain_data::{BlockData, ReceiptLike, TxId, TxIndexEntry, TxLoc};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};

#[test]
fn prune_blocks_removes_old_data() {
    init_stable_state();

    let tx1 = TxId([0x11; 32]);
    let tx2 = TxId([0x22; 32]);
    let tx3 = TxId([0x33; 32]);

    let block1 = make_block(1, tx1);
    let block2 = make_block(2, tx2);
    let block3 = make_block(3, tx3);

    with_state_mut(|state| {
        state.blocks.insert(1, block1);
        state.blocks.insert(2, block2);
        state.blocks.insert(3, block3);
        state.tx_index.insert(tx1, TxIndexEntry { block_number: 1, tx_index: 0 });
        state.tx_index.insert(tx2, TxIndexEntry { block_number: 2, tx_index: 0 });
        state.tx_index.insert(tx3, TxIndexEntry { block_number: 3, tx_index: 0 });
        state.receipts.insert(tx1, fake_receipt(tx1, 1));
        state.receipts.insert(tx2, fake_receipt(tx2, 2));
        state.receipts.insert(tx3, fake_receipt(tx3, 3));
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
        assert!(state.tx_locs.get(&tx1).is_none());
        assert!(state.tx_locs.get(&tx2).is_none());
        assert!(state.tx_locs.get(&tx3).is_some());
    });
}

fn make_block(number: u64, tx_id: TxId) -> BlockData {
    let parent_hash = [0u8; 32];
    let block_hash = [number as u8; 32];
    let tx_list_hash = [number as u8; 32];
    let state_root = [0u8; 32];
    BlockData::new(number, parent_hash, block_hash, number, vec![tx_id], tx_list_hash, state_root)
}

fn fake_receipt(tx_id: TxId, block_number: u64) -> ReceiptLike {
    ReceiptLike {
        tx_id,
        block_number,
        tx_index: 0,
        status: 1,
        gas_used: 0,
        effective_gas_price: 0,
        return_data_hash: [0u8; 32],
        return_data: Vec::new(),
        contract_address: None,
        logs: Vec::new(),
    }
}
