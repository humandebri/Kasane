//! どこで: UPGRADES領域 / 何を: pre/post upgradeの最小退避 / なぜ: Phase0の基本運用

use crate::memory::{get_memory, AppMemoryId, VMem};
use ic_stable_structures::reader::Reader;
use ic_stable_structures::writer::Writer;
use ic_stable_structures::Memory;

const UPGRADE_STATE_VERSION: u32 = 1;

pub fn pre_upgrade() {
    let mut memory: VMem = get_memory(AppMemoryId::Upgrades);
    let mut writer = Writer::new(&mut memory, 0);
    let version_bytes = UPGRADE_STATE_VERSION.to_le_bytes();
    let _ = writer.write(&version_bytes);
}

pub fn post_upgrade() {
    let memory: VMem = get_memory(AppMemoryId::Upgrades);
    if let Some(version) = read_version(&memory) {
        if version != UPGRADE_STATE_VERSION {
            ic_cdk::api::debug_print(format!(
                "upgrade: version mismatch detected (found {}, expected {})",
                version, UPGRADE_STATE_VERSION
            ));
        }
    } else {
        ic_cdk::api::debug_print("upgrade: no persisted state version".to_string());
    }
}

fn read_version(memory: &VMem) -> Option<u32> {
    if memory.size() == 0 {
        return None;
    }
    let mut reader = Reader::new(memory, 0);
    let mut buf = [0u8; 4];
    let read = match reader.read(&mut buf) {
        Ok(value) => value,
        Err(_) => return None,
    };
    if read != 4 {
        return None;
    }
    Some(u32::from_le_bytes(buf))
}
