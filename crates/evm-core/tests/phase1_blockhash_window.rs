//! どこで: Phase1テスト / 何を: BLOCKHASH 256履歴窓の準拠を固定 / なぜ: DB実装差分でEVM仕様逸脱を防ぐため

mod common;

use common::install_contract;
use evm_core::chain::{eth_call_object, CallObjectInput};
use evm_db::chain_data::{BlockData, Head};
use evm_db::stable_state::{init_stable_state, with_state_mut};
use evm_db::Storable;

fn install_block(number: u64, hash: [u8; 32]) {
    with_state_mut(|state| {
        let block = BlockData::new(
            number,
            [0u8; 32],
            hash,
            number,
            1_000_000_000,
            3_000_000,
            0,
            [0u8; 20],
            Vec::new(),
            [0u8; 32],
            [0u8; 32],
        );
        let ptr = state
            .blob_store
            .store_bytes(&block.to_bytes().into_owned())
            .expect("store block");
        state.blocks.insert(number, ptr);
    });
}

fn call_contract(addr: [u8; 20]) -> Vec<u8> {
    evm_core::chain::credit_balance([0x11u8; 20], 1_000_000_000_000_000_000u128)
        .expect("fund caller");
    let out = eth_call_object(CallObjectInput {
        to: Some(addr),
        from: [0x11u8; 20],
        gas_limit: Some(200_000),
        gas_price: Some(500_000_000_000),
        nonce: Some(0),
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: Some(0),
        access_list: Vec::new(),
        value: [0u8; 32],
        data: Vec::new(),
    })
    .expect("eth_call_object");
    out.return_data
}

#[test]
fn blockhash_older_than_256_returns_zero_even_if_db_has_hash() {
    init_stable_state();
    with_state_mut(|state| {
        state.head.set(Head {
            number: 300,
            block_hash: [0xaau8; 32],
            timestamp: 300,
        });
    });
    install_block(1, [0x55u8; 32]);
    // PUSH1 0x01 BLOCKHASH PUSH1 0x00 MSTORE PUSH1 0x20 PUSH1 0x00 RETURN
    let code = [
        0x60, 0x01, 0x40, 0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
    ];
    let contract = [0x42u8; 20];
    install_contract(contract, &code);

    let out = call_contract(contract);
    assert_eq!(out, vec![0u8; 32]);
}

#[test]
fn blockhash_within_256_returns_stored_hash() {
    init_stable_state();
    with_state_mut(|state| {
        state.head.set(Head {
            number: 300,
            block_hash: [0xbbu8; 32],
            timestamp: 300,
        });
    });
    let expected = [0x66u8; 32];
    install_block(299, expected);
    // PUSH2 0x012B BLOCKHASH PUSH1 0x00 MSTORE PUSH1 0x20 PUSH1 0x00 RETURN
    let code = [
        0x61, 0x01, 0x2b, 0x40, 0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
    ];
    let contract = [0x43u8; 20];
    install_contract(contract, &code);

    let out = call_contract(contract);
    assert_eq!(out, expected.to_vec());
}
