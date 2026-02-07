//! どこで: Phase0テスト / 何を: Storable roundtrip検証 / なぜ: 凍結仕様の確認

use evm_db::types::keys::{make_account_key, make_code_key, make_storage_key};
use evm_db::types::values::{AccountVal, CodeVal, U256Val};
use ic_stable_structures::Storable;
use std::borrow::Cow;
use std::panic::catch_unwind;

#[test]
fn storable_roundtrip_keys() {
    let addr = [0x11u8; 20];
    let slot = [0x22u8; 32];
    let code_hash = [0x33u8; 32];

    let account_key = make_account_key(addr);
    let storage_key = make_storage_key(addr, slot);
    let code_key = make_code_key(code_hash);

    let account_bytes = account_key.to_bytes();
    let storage_bytes = storage_key.to_bytes();
    let code_bytes = code_key.to_bytes();

    let account_round = <_ as Storable>::from_bytes(account_bytes);
    let storage_round = <_ as Storable>::from_bytes(storage_bytes);
    let code_round = <_ as Storable>::from_bytes(code_bytes);

    assert_eq!(account_key, account_round);
    assert_eq!(storage_key, storage_round);
    assert_eq!(code_key, code_round);
}

#[test]
fn storable_roundtrip_values() {
    let account_val = AccountVal([0x44u8; 72]);
    let u256_val = U256Val([0x55u8; 32]);
    let code_val = CodeVal(vec![0x66u8; 4]);

    let account_bytes = account_val.to_bytes();
    let u256_bytes = u256_val.to_bytes();
    let code_bytes = code_val.to_bytes();

    let account_round = <_ as Storable>::from_bytes(account_bytes);
    let u256_round = <_ as Storable>::from_bytes(u256_bytes);
    let code_round = <_ as Storable>::from_bytes(code_bytes);

    assert_eq!(account_val, account_round);
    assert_eq!(u256_val, u256_round);
    assert_eq!(code_val, code_round);
}

#[test]
fn account_val_layout_is_fixed() {
    let nonce = 0x1122334455667788u64;
    let balance = [0x11u8; 32];
    let code_hash = [0x22u8; 32];

    let val = AccountVal::from_parts(nonce, balance, code_hash);
    assert_eq!(val.nonce(), nonce);
    assert_eq!(val.balance(), balance);
    assert_eq!(val.code_hash(), code_hash);
}

#[test]
fn storable_rejects_wrong_length() {
    let bad = vec![0u8; 20];
    let result = catch_unwind(|| AccountVal::from_bytes(bad.into()));
    assert!(result.is_ok());
    let value = result.expect("no panic");
    assert_eq!(value, AccountVal([0u8; 72]));
}

#[test]
fn fixed_array_to_bytes_is_borrowed() {
    let addr = [0x11u8; 20];
    let slot = [0x22u8; 32];
    let code_hash = [0x33u8; 32];
    let account_key = make_account_key(addr);
    let storage_key = make_storage_key(addr, slot);
    let code_key = make_code_key(code_hash);
    let account_val = AccountVal([0x44u8; 72]);
    let u256_val = U256Val([0x55u8; 32]);

    assert!(matches!(account_key.to_bytes(), Cow::Borrowed(_)));
    assert!(matches!(storage_key.to_bytes(), Cow::Borrowed(_)));
    assert!(matches!(code_key.to_bytes(), Cow::Borrowed(_)));
    assert!(matches!(account_val.to_bytes(), Cow::Borrowed(_)));
    assert!(matches!(u256_val.to_bytes(), Cow::Borrowed(_)));
}
