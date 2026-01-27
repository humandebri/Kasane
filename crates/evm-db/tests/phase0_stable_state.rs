//! どこで: Phase0テスト / 何を: StableStateの初期化と基本操作 / なぜ: 結線の健全性確認

use evm_db::stable_state::{init_stable_state, with_state_mut};
use evm_db::types::keys::make_account_key;
use evm_db::types::values::AccountVal;

#[test]
fn stable_state_init_and_insert() {
    init_stable_state();
    let addr = [0x77u8; 20];
    let key = make_account_key(addr);
    let val = AccountVal([0x88u8; 72]);

    with_state_mut(|state| {
        state.accounts.insert(key, val);
        let found = state.accounts.get(&key);
        assert_eq!(found, Some(val));
    });
}
