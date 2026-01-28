//! どこで: Phase1テスト / 何を: dev_mint の残高加算 / なぜ: 開発用ファセットの安全性を確認するため

use evm_core::chain;
use evm_db::stable_state::{init_stable_state, with_state};
use evm_db::types::keys::make_account_key;
use revm::primitives::U256;

#[test]
fn dev_mint_increases_balance() {
    init_stable_state();

    let addr = [0x10u8; 20];
    chain::dev_mint(addr, 5).expect("mint");

    let key = make_account_key(addr);
    let account = with_state(|state| state.accounts.get(&key)).expect("account");
    assert_eq!(account.nonce(), 0);
    assert_eq!(U256::from_be_bytes(account.balance()), U256::from(5u64));
    assert_eq!(account.code_hash(), [0u8; 32]);
}
