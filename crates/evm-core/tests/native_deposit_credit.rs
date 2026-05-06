//! どこで: native ICP deposit credit
//! 何を: request_id単位の冪等credit
//! なぜ: ledger pull後の再試行で二重creditを防ぐため

use evm_core::chain;
use evm_db::stable_state::{init_stable_state, with_state};
use evm_db::types::keys::make_account_key;
use revm::primitives::U256;

fn balance(address: [u8; 20]) -> U256 {
    with_state(|state| state.accounts.get(&make_account_key(address)))
        .map(|account| U256::from_be_bytes(account.balance()))
        .unwrap_or(U256::ZERO)
}

#[test]
fn native_deposit_credit_is_idempotent_for_same_payload() {
    init_stable_state();
    let request_id = [0x11u8; 32];
    let recipient = [0x22u8; 20];
    let amount = U256::from(10_000_000_000u128).to_be_bytes();

    chain::credit_native_deposit(request_id, recipient, amount).expect("first credit");
    chain::credit_native_deposit(request_id, recipient, amount).expect("idempotent credit");

    assert_eq!(balance(recipient), U256::from(10_000_000_000u128));
}

#[test]
fn native_deposit_credit_rejects_idempotency_mismatch() {
    init_stable_state();
    let request_id = [0x33u8; 32];
    let recipient = [0x44u8; 20];

    chain::credit_native_deposit(request_id, recipient, U256::from(1u8).to_be_bytes())
        .expect("first credit");
    let err = chain::credit_native_deposit(request_id, recipient, U256::from(2u8).to_be_bytes())
        .expect_err("different amount must fail");

    assert_eq!(err, chain::ChainError::TxAlreadySeen);
    assert_eq!(balance(recipient), U256::from(1u8));
}

#[test]
fn native_deposit_credit_rejects_amount_above_u128() {
    init_stable_state();
    let mut amount = [0u8; 32];
    amount[15] = 1;

    let err = chain::credit_native_deposit([0x55u8; 32], [0x66u8; 20], amount)
        .expect_err("amount above u128 must fail");

    assert_eq!(err, chain::ChainError::MintOverflow);
}
