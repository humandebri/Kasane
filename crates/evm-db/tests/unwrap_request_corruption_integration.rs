//! どこで: unwrap request 統合テスト
//! 何を: StableBTreeMap の生bytes破損注入で decode失敗時の挙動を検証
//! なぜ: 壊れレコード1件で read 経路が trap しない可用性を担保するため

use evm_db::chain_data::{
    TxId, UnwrapDispatchRequest, UnwrapRequestStatus, UNWRAP_DECODE_FAILURE_CODE,
};
use evm_db::memory::WASM_PAGE_SIZE_BYTES;
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{DefaultMemoryImpl, Memory, StableBTreeMap, Storable};
use std::panic::{catch_unwind, AssertUnwindSafe};

type TestMemory = VirtualMemory<DefaultMemoryImpl>;

fn sample_request() -> UnwrapDispatchRequest {
    UnwrapDispatchRequest {
        asset_id: vec![0xB2u8; 12],
        amount: [0xC3u8; 32],
        recipient: vec![0xD4u8; 20],
        status: UnwrapRequestStatus::Queued,
        ledger_tx_id: Some(vec![0xE5u8; 16]),
        error_code: Some("wrap.integration.sample".to_string()),
        updated_at: 123_456_799,
        transfer_created_at_time: 123_456_800,
    }
}

fn find_unique_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    let mut first = None;
    let mut start = 0usize;
    while start + needle.len() <= haystack.len() {
        let Some(pos) = haystack[start..]
            .windows(needle.len())
            .position(|window| window == needle)
        else {
            break;
        };
        let found = start + pos;
        if first.is_some() {
            return None;
        }
        first = Some(found);
        start = found + 1;
    }
    first
}

#[test]
fn unwrap_request_map_get_survives_raw_stable_bytes_corruption() {
    let manager = MemoryManager::init(DefaultMemoryImpl::default());
    let memory: TestMemory = manager.get(MemoryId::new(0));
    let mut map: StableBTreeMap<TxId, UnwrapDispatchRequest, TestMemory> =
        StableBTreeMap::init(memory.clone());

    let request_id = TxId([0xABu8; 32]);
    let request = sample_request();
    let encoded = request.to_bytes().into_owned();
    map.insert(request_id, request);

    let pages = memory.size();
    assert!(pages > 0, "stable memory must contain map bytes");
    let mut dump = vec![0u8; (pages * WASM_PAGE_SIZE_BYTES) as usize];
    memory.read(0, &mut dump);

    let encoded_offset = find_unique_subsequence(&dump, &encoded)
        .expect("encoded unwrap request bytes must appear exactly once");
    let checksum_last = encoded_offset + encoded.len() - 1;
    let corrupted = dump[checksum_last] ^ 0x01;
    memory.write(checksum_last as u64, &[corrupted]);

    let out = catch_unwind(AssertUnwindSafe(|| map.get(&request_id)));
    assert!(out.is_ok(), "raw bytes corruption must not panic on get");
    let decoded = out
        .expect("map.get should not panic")
        .expect("request entry should still exist");
    assert_eq!(decoded.status, UnwrapRequestStatus::DispatchFailed);
    assert_eq!(
        decoded.error_code.as_deref(),
        Some(UNWRAP_DECODE_FAILURE_CODE)
    );
    assert_eq!(decoded.ledger_tx_id, None);
}
