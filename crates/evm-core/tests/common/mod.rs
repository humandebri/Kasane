//! どこで: evm-core integration tests / 何を: テスト補助関数 / なぜ: 重複を減らし変更点を1箇所に集約するため

#![allow(dead_code)]

use evm_core::hash;
use evm_core::tx_decode::{encode_ic_synthetic_input, IcSyntheticTxInput};
use evm_db::chain_data::{ReceiptLike, TxId};
use evm_db::stable_state::with_state_mut;
use evm_db::types::keys::{make_account_key, make_code_key};
use evm_db::types::values::{AccountVal, CodeVal};

pub fn build_ic_tx_bytes(
    to: [u8; 20],
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) -> Vec<u8> {
    encode_ic_synthetic_input(&build_ic_tx_input(
        to,
        nonce,
        max_fee_per_gas,
        max_priority_fee_per_gas,
    ))
}

pub fn build_ic_tx_input(
    to: [u8; 20],
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) -> IcSyntheticTxInput {
    IcSyntheticTxInput {
        to: Some(to),
        value: [0u8; 32],
        gas_limit: 50_000,
        nonce,
        max_fee_per_gas,
        max_priority_fee_per_gas,
        data: Vec::new(),
    }
}

pub fn build_default_ic_tx_bytes(nonce: u64) -> Vec<u8> {
    build_ic_tx_bytes([0x10u8; 20], nonce, 2_000_000_000, 1_000_000_000)
}

pub fn build_default_ic_tx_input(nonce: u64) -> IcSyntheticTxInput {
    build_ic_tx_input([0x10u8; 20], nonce, 2_000_000_000, 1_000_000_000)
}

pub fn build_zero_to_ic_tx_bytes(
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) -> Vec<u8> {
    build_ic_tx_bytes([0u8; 20], nonce, max_fee_per_gas, max_priority_fee_per_gas)
}

pub fn build_zero_to_ic_tx_input(
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) -> IcSyntheticTxInput {
    build_ic_tx_input([0u8; 20], nonce, max_fee_per_gas, max_priority_fee_per_gas)
}

pub fn install_contract(address: [u8; 20], code: &[u8]) {
    let code_hash = hash::keccak256(code);
    with_state_mut(|state| {
        let account_key = make_account_key(address);
        let account = AccountVal::from_parts(0, [0u8; 32], code_hash);
        let code_key = make_code_key(code_hash);
        state.accounts.insert(account_key, account);
        state.codes.insert(code_key, CodeVal(code.to_vec()));
    });
}

pub fn fund_account(address: [u8; 20], amount: u128) {
    evm_core::chain::credit_balance(address, amount).expect("fund account");
}

pub fn execute_ic_tx_via_produce(
    caller_principal: Vec<u8>,
    canister_id: Vec<u8>,
    tx: IcSyntheticTxInput,
) -> (TxId, ReceiptLike) {
    let tx_id = evm_core::chain::submit_tx_in(evm_core::chain::TxIn::IcSynthetic {
        caller_principal,
        canister_id,
        tx,
    })
    .expect("submit");
    let outcome = evm_core::chain::produce_block(1).expect("produce");
    assert_eq!(outcome.block.tx_ids.len(), 1);
    assert_eq!(outcome.block.tx_ids[0], tx_id);
    let receipt = evm_core::chain::get_receipt(&tx_id).expect("receipt");
    (tx_id, receipt)
}
