//! どこで: Phase1テスト / 何を: ハッシュ決定性 / なぜ: 再現性を保証するため

use evm_core::hash::{block_hash, keccak256, tx_id, tx_list_hash};

#[test]
fn tx_id_is_deterministic() {
    let a = tx_id(b"hello");
    let b = tx_id(b"hello");
    assert_eq!(a, b);
}

#[test]
fn tx_list_hash_depends_on_order() {
    let a = tx_id(b"a");
    let b = tx_id(b"b");
    let list1 = tx_list_hash(&[a, b]);
    let list2 = tx_list_hash(&[b, a]);
    assert_ne!(list1, list2);
}

#[test]
fn block_hash_is_deterministic() {
    let parent = keccak256(b"parent");
    let tx_list = keccak256(b"txs");
    let state_root = keccak256(b"state");
    let h1 = block_hash(parent, 1, 1, tx_list, state_root);
    let h2 = block_hash(parent, 1, 1, tx_list, state_root);
    assert_eq!(h1, h2);
}
