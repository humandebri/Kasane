//! どこで: evm-core query系テスト
//! 何を: wrap precompile が eth_call / estimateGas で利用可能かを固定
//! なぜ: 実送信だけ成功して query 見積もりが壊れる回帰を防ぐため

use evm_core::chain::{self, CallObjectInput};
use evm_core::hash;
use evm_core::wrap_precompile::WRAP_PRECOMPILE_ADDRESS;
use evm_db::stable_state::init_stable_state;

fn abi_word_from_u64(value: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[24..].copy_from_slice(&value.to_be_bytes());
    out
}

fn abi_word_from_u128(value: u128) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(&value.to_be_bytes());
    out
}

fn encode_dynamic_bytes(bytes: &[u8]) -> Vec<u8> {
    let padded = bytes.len().div_ceil(32) * 32;
    let mut out = Vec::with_capacity(32 + padded);
    out.extend_from_slice(&abi_word_from_u64(bytes.len() as u64));
    out.extend_from_slice(bytes);
    out.resize(32 + padded, 0);
    out
}

fn encode_unwrap_input() -> Vec<u8> {
    let vault = vec![0x11u8, 0x22, 0x33];
    let asset = vec![0x44u8, 0x55, 0x66];
    let recipient = vec![0x77u8, 0x88, 0x99];
    let vault_tail = encode_dynamic_bytes(&vault);
    let asset_tail = encode_dynamic_bytes(&asset);
    let recipient_tail = encode_dynamic_bytes(&recipient);
    let head_size = 32 * 6;

    let mut out =
        Vec::with_capacity(head_size + vault_tail.len() + asset_tail.len() + recipient_tail.len());
    out.extend_from_slice(&abi_word_from_u64(head_size as u64));
    out.extend_from_slice(&abi_word_from_u64((head_size + vault_tail.len()) as u64));
    out.extend_from_slice(&abi_word_from_u128(1_000_000_000_000u128));
    out.extend_from_slice(&abi_word_from_u64(
        (head_size + vault_tail.len() + asset_tail.len()) as u64,
    ));
    out.extend_from_slice(&abi_word_from_u64(7));
    out.extend_from_slice(&abi_word_from_u64(u64::MAX));
    out.extend_from_slice(&vault_tail);
    out.extend_from_slice(&asset_tail);
    out.extend_from_slice(&recipient_tail);
    out
}

fn build_call_input(data: Vec<u8>) -> CallObjectInput {
    CallObjectInput {
        to: Some(WRAP_PRECOMPILE_ADDRESS.into_array()),
        from: [0x31u8; 20],
        gas_limit: Some(300_000),
        gas_price: Some(500_000_000_000),
        nonce: Some(0),
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: Some(0),
        access_list: Vec::new(),
        value: [0u8; 32],
        data,
    }
}

#[test]
fn wrap_precompile_eth_call_object_succeeds_in_query_path() {
    init_stable_state();
    let caller = [0x31u8; 20];
    chain::credit_balance(caller, 1_000_000_000_000_000_000u128).expect("fund caller");
    let input = encode_unwrap_input();
    let out = chain::eth_call_object(build_call_input(input.clone())).expect("eth_call_object");

    let mut expected_hash_input = Vec::with_capacity(caller.len() + input.len());
    expected_hash_input.extend_from_slice(&caller);
    expected_hash_input.extend_from_slice(&input);
    assert_eq!(out.status, 1);
    assert_eq!(
        out.return_data,
        hash::keccak256(&expected_hash_input).to_vec()
    );
    assert!(out.revert_data.is_none());
}

#[test]
fn wrap_precompile_eth_estimate_gas_succeeds_in_query_path() {
    init_stable_state();
    let caller = [0x31u8; 20];
    chain::credit_balance(caller, 1_000_000_000_000_000_000u128).expect("fund caller");
    let input = encode_unwrap_input();
    let gas = chain::eth_estimate_gas_object(build_call_input(input)).expect("estimate gas");

    assert!(gas > 0);
    assert!(gas <= 300_000);
}
