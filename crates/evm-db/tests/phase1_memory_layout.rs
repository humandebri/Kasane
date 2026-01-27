//! どこで: Phase1テスト / 何を: MemoryId拡張の固定 / なぜ: レイアウト破壊を防ぐため

use evm_db::memory::AppMemoryId;

#[test]
fn phase1_memory_ids_are_fixed() {
    assert_eq!(AppMemoryId::QueueMeta.as_u8(), 6);
    assert_eq!(AppMemoryId::Queue.as_u8(), 7);
    assert_eq!(AppMemoryId::SeenTx.as_u8(), 8);
    assert_eq!(AppMemoryId::TxStore.as_u8(), 9);
    assert_eq!(AppMemoryId::TxIndex.as_u8(), 10);
    assert_eq!(AppMemoryId::Receipts.as_u8(), 11);
    assert_eq!(AppMemoryId::Blocks.as_u8(), 12);
    assert_eq!(AppMemoryId::Head.as_u8(), 13);
}
