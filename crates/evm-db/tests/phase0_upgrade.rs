//! どこで: Phase0テスト / 何を: UPGRADES領域のversion書き込み / なぜ: upgrade耐性の確認

use evm_db::memory::{get_memory, AppMemoryId};
use evm_db::upgrade::pre_upgrade;
use ic_stable_structures::reader::Reader;

#[test]
fn upgrade_writes_version() {
    pre_upgrade();
    let memory = get_memory(AppMemoryId::Upgrades);
    let mut reader = Reader::new(&memory, 0);
    let mut buf = [0u8; 4];
    let read = reader.read(&mut buf).expect("read version");
    assert_eq!(read, 4);
    let version = u32::from_le_bytes(buf);
    assert_eq!(version, 1);
}
