//! どこで: Phase1テスト / 何を: ハッシュ決定性 / なぜ: 再現性を保証するため

use evm_core::hash::{block_hash, keccak256, stored_tx_id, tx_list_hash};
use evm_core::state_root::{compute_block_change_hash, compute_state_root_from_changes, empty_tx_change_hash};
use evm_db::chain_data::TxKind;

#[test]
fn tx_id_is_deterministic() {
    let a = stored_tx_id(TxKind::EthSigned, b"hello", None, None, None);
    let b = stored_tx_id(TxKind::EthSigned, b"hello", None, None, None);
    assert_eq!(a, b);
}

#[test]
fn tx_list_hash_depends_on_order() {
    let a = stored_tx_id(TxKind::EthSigned, b"a", None, None, None);
    let b = stored_tx_id(TxKind::EthSigned, b"b", None, None, None);
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

#[test]
fn state_root_changes_with_block_changes() {
    let prev = keccak256(b"prev");
    let tx_list = keccak256(b"txs");
    let block_change_a = compute_block_change_hash(&[keccak256(b"a")]);
    let block_change_b = compute_block_change_hash(&[keccak256(b"b")]);
    let root_a = compute_state_root_from_changes(prev, 1, tx_list, block_change_a);
    let root_b = compute_state_root_from_changes(prev, 1, tx_list, block_change_b);
    assert_ne!(root_a, root_b);
}

#[test]
fn empty_tx_change_hash_is_stable() {
    let a = empty_tx_change_hash();
    let b = empty_tx_change_hash();
    assert_eq!(a, b);
}
