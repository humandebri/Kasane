//! どこで: Phase1テスト / 何を: RevmStableDbの基本読み取り / なぜ: 型変換の確認

use evm_core::revm_db::RevmStableDb;
use evm_db::stable_state::init_stable_state;
use evm_db::types::keys::{make_account_key, make_storage_key};
use evm_db::types::values::{AccountVal, U256Val};
use evm_db::stable_state::with_state_mut;
use revm::primitives::{address, U256};
use revm::Database;

#[test]
fn revm_db_basic_and_storage() {
    init_stable_state();

    let addr = address!("0x0102030405060708090a0b0c0d0e0f1011121314");
    let addr_bytes = addr.as_ref();
    let mut addr20 = [0u8; 20];
    addr20.copy_from_slice(addr_bytes);

    let key = make_account_key(addr20);
    let balance = U256::from(5u64).to_be_bytes();
    let code_hash = [0x11u8; 32];
    let account = AccountVal::from_parts(7, balance, code_hash);

    let slot = [0x22u8; 32];
    let storage_key = make_storage_key(addr20, slot);

    with_state_mut(|state| {
        state.accounts.insert(key, account);
        state.storage.insert(storage_key, U256Val([0x33u8; 32]));
    });

    let mut db = RevmStableDb;
    let info = db.basic(addr).expect("basic").expect("exists");
    assert_eq!(info.nonce, 7);
    assert_eq!(info.balance, U256::from(5u64));

    let storage = db
        .storage(addr, U256::from_be_bytes(slot))
        .expect("storage");
    assert_eq!(storage, U256::from_be_bytes([0x33u8; 32]));
}
