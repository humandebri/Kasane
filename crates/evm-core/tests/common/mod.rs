//! どこで: evm-core integration tests / 何を: テスト補助関数 / なぜ: 重複を減らし変更点を1箇所に集約するため

#![allow(dead_code)]

use evm_core::hash;
use evm_db::stable_state::with_state_mut;
use evm_db::types::keys::{make_account_key, make_code_key};
use evm_db::types::values::{AccountVal, CodeVal};

pub fn build_ic_tx_bytes(
    to: [u8; 20],
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) -> Vec<u8> {
    let value = [0u8; 32];
    let gas_limit = 50_000u64.to_be_bytes();
    let nonce = nonce.to_be_bytes();
    let max_fee = max_fee_per_gas.to_be_bytes();
    let max_priority = max_priority_fee_per_gas.to_be_bytes();
    let data: Vec<u8> = Vec::new();
    let data_len = u32::try_from(data.len()).unwrap_or(0).to_be_bytes();
    let mut out = Vec::with_capacity(1 + 20 + 32 + 8 + 8 + 16 + 16 + 4 + data.len());
    out.push(2u8);
    out.extend_from_slice(&to);
    out.extend_from_slice(&value);
    out.extend_from_slice(&gas_limit);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&max_fee);
    out.extend_from_slice(&max_priority);
    out.extend_from_slice(&data_len);
    out.extend_from_slice(&data);
    out
}

pub fn build_default_ic_tx_bytes(nonce: u64) -> Vec<u8> {
    build_ic_tx_bytes([0x10u8; 20], nonce, 2_000_000_000, 1_000_000_000)
}

pub fn build_zero_to_ic_tx_bytes(
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) -> Vec<u8> {
    build_ic_tx_bytes([0u8; 20], nonce, max_fee_per_gas, max_priority_fee_per_gas)
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
