//! どこで: Phase1テスト / 何を: MemoryId拡張の固定 / なぜ: レイアウト破壊を防ぐため

use evm_db::memory::AppMemoryId;
use verified_core::stable_namespace::stable_tx_namespace_disjoint_raw;

#[test]
fn chain_data_memory_ids_are_fixed() {
    assert_eq!(AppMemoryId::QueueMeta.as_u8(), 6);
    assert_eq!(AppMemoryId::Queue.as_u8(), 7);
    assert_eq!(AppMemoryId::SeenTx.as_u8(), 8);
    assert_eq!(AppMemoryId::TxStore.as_u8(), 9);
    assert_eq!(AppMemoryId::TxIndex.as_u8(), 10);
    assert_eq!(AppMemoryId::Receipts.as_u8(), 11);
    assert_eq!(AppMemoryId::Blocks.as_u8(), 12);
    assert_eq!(AppMemoryId::Head.as_u8(), 13);
    assert_eq!(AppMemoryId::ChainState.as_u8(), 14);
    assert_eq!(AppMemoryId::CallerNonces.as_u8(), 15);
    assert_eq!(AppMemoryId::TxLocs.as_u8(), 16);
    assert_eq!(AppMemoryId::NativeCreditRecords.as_u8(), 59);
    assert_eq!(AppMemoryId::WrapRequests.as_u8(), 60);
    assert_eq!(AppMemoryId::WrapQueue.as_u8(), 61);
    assert_eq!(AppMemoryId::WrapQueueMeta.as_u8(), 62);
    assert_eq!(AppMemoryId::WrapAllowedAssets.as_u8(), 63);
    assert_eq!(AppMemoryId::WrapFeePolicy.as_u8(), 64);
    assert_eq!(AppMemoryId::WrapEvmConfig.as_u8(), 65);
    assert_eq!(AppMemoryId::WrapNativeLedgerCanister.as_u8(), 66);
    assert_eq!(AppMemoryId::WrapPendingSubmissions.as_u8(), 67);
    assert_eq!(AppMemoryId::QueryPrecompileAllowlist.as_u8(), 68);
    assert_eq!(AppMemoryId::EvmStateEpoch.as_u8(), 69);
}

#[test]
fn tx_related_memory_ids_are_disjoint() {
    assert!(stable_tx_namespace_disjoint_raw(
        u64::from(AppMemoryId::SeenTx.as_u8()),
        u64::from(AppMemoryId::TxStore.as_u8()),
        u64::from(AppMemoryId::TxIndex.as_u8()),
        u64::from(AppMemoryId::Receipts.as_u8()),
        u64::from(AppMemoryId::TxLocs.as_u8()),
        u64::from(AppMemoryId::TxLocsV3.as_u8()),
        u64::from(AppMemoryId::InternalTraces.as_u8()),
    ));
}
