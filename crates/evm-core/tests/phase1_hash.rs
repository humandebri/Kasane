//! どこで: Phase1テスト / 何を: ハッシュ決定性 / なぜ: 再現性を保証するため

use evm_core::hash::{block_hash, keccak256, stored_tx_id, tx_list_hash};
use evm_core::state_root::compute_state_root_with;
use evm_db::chain_data::TxKind;
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
use evm_db::types::keys::{make_account_key, make_storage_key};
use evm_db::types::values::{AccountVal, U256Val};

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
fn empty_state_root_matches_ethereum_empty_trie() {
    init_stable_state();
    let root = with_state(compute_state_root_with);
    assert_eq!(
        hex32(root),
        "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
    );
}

#[test]
fn state_root_is_deterministic_for_same_state() {
    init_stable_state();
    let addr = [0x11u8; 20];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr),
            AccountVal::from_parts(1, [0u8; 32], [0u8; 32]),
        );
        state.storage.insert(
            make_storage_key(addr, [0x01u8; 32]),
            U256Val::new([0x0au8; 32]),
        );
    });
    let root_a = with_state(compute_state_root_with);
    let root_b = with_state(compute_state_root_with);
    assert_eq!(root_a, root_b);
}

#[test]
fn state_root_is_stable_against_storage_insertion_order() {
    init_stable_state();
    let addr = [0x22u8; 20];
    let slot_a = [0x01u8; 32];
    let slot_b = [0x02u8; 32];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr),
            AccountVal::from_parts(1, [0u8; 32], [0u8; 32]),
        );
        state
            .storage
            .insert(make_storage_key(addr, slot_a), U256Val::new([0x0au8; 32]));
        state
            .storage
            .insert(make_storage_key(addr, slot_b), U256Val::new([0x0bu8; 32]));
    });
    let root_a = with_state(compute_state_root_with);
    with_state_mut(|state| {
        state.storage.remove(&make_storage_key(addr, slot_a));
        state.storage.remove(&make_storage_key(addr, slot_b));
        state
            .storage
            .insert(make_storage_key(addr, slot_b), U256Val::new([0x0bu8; 32]));
        state
            .storage
            .insert(make_storage_key(addr, slot_a), U256Val::new([0x0au8; 32]));
    });
    let root_b = with_state(compute_state_root_with);
    assert_eq!(root_a, root_b);
}

#[test]
fn state_root_changes_when_storage_changes() {
    init_stable_state();
    let addr = [0x33u8; 20];
    let slot = [0x03u8; 32];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr),
            AccountVal::from_parts(1, [0u8; 32], [0u8; 32]),
        );
        state
            .storage
            .insert(make_storage_key(addr, slot), U256Val::new([0x0au8; 32]));
    });
    let root_a = with_state(compute_state_root_with);
    with_state_mut(|state| {
        state
            .storage
            .insert(make_storage_key(addr, slot), U256Val::new([0x0cu8; 32]));
    });
    let root_b = with_state(compute_state_root_with);
    assert_ne!(root_a, root_b);
}

#[test]
fn zero_code_hash_account_is_treated_as_empty_code_hash() {
    init_stable_state();
    let addr = [0x44u8; 20];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr),
            AccountVal::from_parts(0, [0u8; 32], [0u8; 32]),
        );
    });
    let root = with_state(compute_state_root_with);
    assert_eq!(
        hex32(root),
        "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
    );
}

fn hex32(value: [u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in value {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
