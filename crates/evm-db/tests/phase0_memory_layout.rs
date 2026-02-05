//! どこで: Phase0テスト / 何を: MemoryIdの固定確認 / なぜ: レイアウト破壊を防ぐため

use evm_db::memory::AppMemoryId;

#[test]
fn memory_id_layout_is_frozen() {
    assert_eq!(AppMemoryId::Upgrades.as_u8(), 0);
    assert_eq!(AppMemoryId::Meta.as_u8(), 1);
    assert_eq!(AppMemoryId::Accounts.as_u8(), 2);
    assert_eq!(AppMemoryId::Storage.as_u8(), 3);
    assert_eq!(AppMemoryId::Codes.as_u8(), 4);
    assert_eq!(AppMemoryId::StateAux.as_u8(), 5);
    assert_eq!(AppMemoryId::CorruptLog.as_u8(), 34);
    assert_eq!(AppMemoryId::OpsConfig.as_u8(), 35);
    assert_eq!(AppMemoryId::OpsState.as_u8(), 36);
    assert_eq!(AppMemoryId::Reserved37.as_u8(), 37);
    assert_eq!(AppMemoryId::Reserved38.as_u8(), 38);
    assert_eq!(AppMemoryId::OpsMetrics.as_u8(), 39);
    assert_eq!(AppMemoryId::Reserved40.as_u8(), 40);
}
