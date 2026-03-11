//! どこで: evm-core query系テスト
//! 何を: wrap precompile が eth_call / estimateGas で利用可能かを固定
//! なぜ: 実送信だけ成功して query 見積もりが壊れる回帰を防ぐため

use evm_core::chain::{self, CallObjectInput};
use evm_core::wrap_precompile::WRAP_PRECOMPILE_ADDRESS;
use evm_db::stable_state::init_stable_state;

fn encode_unwrap_input() -> Vec<u8> {
    let asset = vec![0x44u8, 0x55, 0x66];
    let recipient = vec![0x77u8, 0x88, 0x99];

    fn encode_principal(bytes: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; 30];
        out[0] = bytes.len() as u8;
        out[1..1 + bytes.len()].copy_from_slice(bytes);
        out
    }

    let mut amount = [0u8; 32];
    amount[16..].copy_from_slice(&1_000_000_000_000u128.to_be_bytes());
    let mut out = Vec::with_capacity(93);
    out.push(1);
    out.extend_from_slice(&encode_principal(&asset));
    out.extend_from_slice(&amount);
    out.extend_from_slice(&encode_principal(&recipient));
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
    let out = chain::eth_call_object(build_call_input(input)).expect("eth_call_object");

    assert_eq!(out.status, 1);
    assert!(out.return_data.is_empty());
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
