//! どこで: 破損デコード検知 / 何を: 回数と最後のタグをStableに記録 / なぜ: 復旧の検知性を確保するため

use crate::memory::{get_memory, AppMemoryId, VMem};
use ic_stable_structures::reader::Reader;
use ic_stable_structures::writer::Writer;
use ic_stable_structures::Memory;
use std::cell::Cell;

const MAGIC: [u8; 4] = *b"CRPT";
const VERSION: u32 = 1;
const HEADER_SIZE: u64 = 8;
const OFFSET_COUNT: u64 = HEADER_SIZE;
const OFFSET_LAST_TS: u64 = HEADER_SIZE + 8;
const OFFSET_LAST_TAG: u64 = HEADER_SIZE + 16;

thread_local! {
    static LAST_RECORDED_TS: Cell<u64> = Cell::new(0);
}

pub fn record_corrupt(tag: &'static [u8]) {
    if !is_replicated_execution() {
        return;
    }
    let mut memory: VMem = get_memory(AppMemoryId::CorruptLog);
    let last_ts = current_time_nanos();
    let already_recorded = LAST_RECORDED_TS.with(|cell| {
        let previous = cell.get();
        if previous == last_ts {
            true
        } else {
            cell.set(last_ts);
            false
        }
    });
    if already_recorded {
        return;
    }
    ensure_header(&mut memory);
    let count = read_u64(&memory, OFFSET_COUNT).saturating_add(1);
    let tag_slot = encode_tag_slot(tag);
    write_u64(&mut memory, OFFSET_COUNT, count);
    write_u64(&mut memory, OFFSET_LAST_TS, last_ts);
    write_bytes(&mut memory, OFFSET_LAST_TAG, &tag_slot);
}

pub fn read_corrupt_count() -> u64 {
    let memory: VMem = get_memory(AppMemoryId::CorruptLog);
    if !header_matches(&memory) {
        return 0;
    }
    read_u64(&memory, OFFSET_COUNT)
}

pub fn read_last_corrupt_ts() -> u64 {
    let memory: VMem = get_memory(AppMemoryId::CorruptLog);
    if !header_matches(&memory) {
        return 0;
    }
    read_u64(&memory, OFFSET_LAST_TS)
}

pub fn read_last_corrupt_tag_hash() -> [u8; 32] {
    read_last_corrupt_tag()
}

pub fn read_last_corrupt_tag() -> [u8; 32] {
    let memory: VMem = get_memory(AppMemoryId::CorruptLog);
    if !header_matches(&memory) {
        return [0u8; 32];
    }
    read_array_32(&memory, OFFSET_LAST_TAG)
}

fn encode_tag_slot(tag: &'static [u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let len = tag.len().min(32);
    out[..len].copy_from_slice(&tag[..len]);
    out
}

fn current_time_nanos() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        ic_cdk::api::time()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0
    }
}

fn is_replicated_execution() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        ic_cdk::api::in_replicated_execution()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

fn read_u64(memory: &VMem, offset: u64) -> u64 {
    if memory.size() == 0 {
        return 0;
    }
    if !header_matches(memory) {
        return 0;
    }
    let mut reader = Reader::new(memory, offset);
    let mut buf = [0u8; 8];
    let read = reader.read(&mut buf).unwrap_or(0);
    if read != 8 {
        return 0;
    }
    u64::from_be_bytes(buf)
}

fn read_array_32(memory: &VMem, offset: u64) -> [u8; 32] {
    if memory.size() == 0 {
        return [0u8; 32];
    }
    if !header_matches(memory) {
        return [0u8; 32];
    }
    let mut reader = Reader::new(memory, offset);
    let mut buf = [0u8; 32];
    let read = reader.read(&mut buf).unwrap_or(0);
    if read != 32 {
        return [0u8; 32];
    }
    buf
}

fn write_u64(memory: &mut VMem, offset: u64, value: u64) {
    let mut writer = Writer::new(memory, offset);
    let _ = writer.write(&value.to_be_bytes());
}

fn write_bytes(memory: &mut VMem, offset: u64, bytes: &[u8]) {
    let mut writer = Writer::new(memory, offset);
    let _ = writer.write(bytes);
}

fn ensure_header(memory: &mut VMem) {
    if header_matches(memory) {
        return;
    }
    let mut writer = Writer::new(memory, 0);
    let _ = writer.write(&MAGIC);
    let _ = writer.write(&VERSION.to_be_bytes());
}

fn header_matches(memory: &VMem) -> bool {
    if memory.size() == 0 {
        return false;
    }
    let mut reader = Reader::new(memory, 0);
    let mut magic = [0u8; 4];
    let mut version = [0u8; 4];
    if reader.read(&mut magic).unwrap_or(0) != 4 {
        return false;
    }
    if reader.read(&mut version).unwrap_or(0) != 4 {
        return false;
    }
    magic == MAGIC && u32::from_be_bytes(version) == VERSION
}
