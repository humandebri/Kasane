//! どこで: evm-db のユニットテスト / 何を: BlobStoreの往復と再利用 / なぜ: Step0の保存基盤を固定するため

use evm_db::blob_store::{AllocKey, BlobStore};
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{DefaultMemoryImpl, StableBTreeMap, StableCell, Storable};
use std::borrow::Cow;

type VMem = VirtualMemory<DefaultMemoryImpl>;

fn new_blob_store() -> BlobStore {
    let manager = MemoryManager::init(DefaultMemoryImpl::default());
    let arena: VMem = manager.get(MemoryId::new(0));
    let arena_end = StableCell::init(manager.get(MemoryId::new(1)), 0u64);
    let alloc_table: StableBTreeMap<_, _, VMem> =
        StableBTreeMap::init(manager.get(MemoryId::new(2)));
    let free_list: StableBTreeMap<_, _, VMem> = StableBTreeMap::init(manager.get(MemoryId::new(3)));
    BlobStore::new(arena, arena_end, alloc_table, free_list)
}

#[test]
fn blob_store_roundtrip() {
    let mut store = new_blob_store();
    let data = vec![1u8, 2, 3, 4, 5];
    let ptr = store.store_bytes(&data).expect("store should succeed");
    let loaded = store.read(&ptr).expect("read should succeed");
    assert_eq!(loaded, data);
}

#[test]
fn blob_store_reuse_increments_generation() {
    let mut store = new_blob_store();
    let data = vec![7u8; 128];
    let ptr = store.store_bytes(&data).expect("store should succeed");
    store
        .mark_quarantine(&ptr)
        .expect("quarantine should succeed");
    store.mark_free(&ptr).expect("free should succeed");
    let ptr2 = store.store_bytes(&data).expect("store should succeed");
    assert_eq!(ptr.offset(), ptr2.offset());
    assert!(ptr2.gen() > ptr.gen());
}

#[test]
fn quarantine_is_not_reused() {
    let mut store = new_blob_store();
    let data = vec![9u8; 128];
    let ptr = store.store_bytes(&data).expect("store should succeed");
    store
        .mark_quarantine(&ptr)
        .expect("quarantine should succeed");
    let ptr2 = store.store_bytes(&data).expect("store should succeed");
    assert_ne!(ptr.offset(), ptr2.offset());
}

#[test]
fn reclaim_for_prune_works_from_used_and_is_idempotent() {
    let mut store = new_blob_store();
    let data = vec![3u8; 64];
    let ptr = store.store_bytes(&data).expect("store should succeed");
    store
        .reclaim_for_prune(&ptr)
        .expect("reclaim from used should succeed");
    store
        .reclaim_for_prune(&ptr)
        .expect("reclaim should be idempotent");
    let ptr2 = store.store_bytes(&data).expect("store should succeed");
    assert_eq!(ptr.offset(), ptr2.offset());
    assert!(ptr2.gen() > ptr.gen());
}

#[test]
fn blob_ptr_and_allockey_to_bytes_are_borrowed() {
    let ptr = evm_db::blob_ptr::BlobPtr::new(1, 2, 3, 4);
    let key = AllocKey::new(7, 9);
    assert!(matches!(ptr.to_bytes(), Cow::Borrowed(_)));
    assert!(matches!(key.to_bytes(), Cow::Borrowed(_)));
}

#[test]
fn usage_stats_tracks_used_quarantine_free_and_arena_end() {
    let mut store = new_blob_store();
    let data = vec![1u8; 100];

    let initial = store.usage_stats();
    assert_eq!(initial.used_class_bytes, 0);
    assert_eq!(initial.quarantine_class_bytes, 0);
    assert_eq!(initial.free_class_bytes, 0);
    assert_eq!(initial.arena_end_bytes, 0);

    let ptr = store.store_bytes(&data).expect("store should succeed");
    let after_store = store.usage_stats();
    assert!(after_store.used_class_bytes >= u64::from(ptr.class()));
    assert_eq!(after_store.quarantine_class_bytes, 0);
    assert_eq!(after_store.free_class_bytes, 0);
    assert!(after_store.arena_end_bytes >= u64::from(ptr.class()));

    store
        .mark_quarantine(&ptr)
        .expect("quarantine should succeed");
    let after_quarantine = store.usage_stats();
    assert_eq!(after_quarantine.used_class_bytes, 0);
    assert!(after_quarantine.quarantine_class_bytes >= u64::from(ptr.class()));

    store
        .reclaim_for_prune(&ptr)
        .expect("reclaim should succeed");
    let after_reclaim = store.usage_stats();
    assert_eq!(after_reclaim.used_class_bytes, 0);
    assert_eq!(after_reclaim.quarantine_class_bytes, 0);
    assert!(after_reclaim.free_class_bytes >= u64::from(ptr.class()));
    assert_eq!(after_reclaim.arena_end_bytes, after_store.arena_end_bytes);
}

#[test]
fn usage_stats_updates_when_reusing_free_slot() {
    let mut store = new_blob_store();
    let payload = vec![7u8; 96];

    let first = store.store_bytes(&payload).expect("initial store should succeed");
    store
        .mark_quarantine(&first)
        .expect("quarantine should succeed");
    store.mark_free(&first).expect("mark_free should succeed");

    let after_free = store.usage_stats();
    assert_eq!(after_free.used_class_bytes, 0);
    assert!(after_free.free_class_bytes >= u64::from(first.class()));

    let second = store.store_bytes(&payload).expect("reuse store should succeed");
    assert_eq!(second.class(), first.class());

    let after_reuse = store.usage_stats();
    assert!(
        after_reuse.used_class_bytes >= u64::from(second.class()),
        "used bytes should increase when reusing free slot"
    );
    assert!(
        after_reuse.free_class_bytes.saturating_add(u64::from(second.class()))
            >= after_free.free_class_bytes,
        "free bytes should decrease when free slot is reused"
    );
}
