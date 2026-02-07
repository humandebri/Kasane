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
fn blob_ptr_and_allockey_to_bytes_are_borrowed() {
    let ptr = evm_db::blob_ptr::BlobPtr::new(1, 2, 3, 4);
    let key = AllocKey::new(7, 9);
    assert!(matches!(ptr.to_bytes(), Cow::Borrowed(_)));
    assert!(matches!(key.to_bytes(), Cow::Borrowed(_)));
}
