//! どこで: evm-core query系テスト
//! 何を: Kasane precompile 群が eth_call / estimateGas で利用可能かを固定
//! なぜ: 実送信だけ成功して query 見積もりが壊れる回帰を防ぐため

use evm_core::chain::{self, CallObjectInput, ChainError};
use evm_core::hash;
use evm_core::kasane_precompiles::{
    precompile_allow_key, ICP_QUERY_PRECOMPILE_ADDRESS, ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS,
    WRAP_PRECOMPILE_ADDRESS,
};
use evm_core::revm_exec::ExecError;
use evm_core::tx_decode::IcSyntheticTxInput;
use evm_db::chain_data::{constants::CHAIN_ID, RuntimeConfigV1};
use evm_db::stable_state::{init_stable_state, set_runtime_config, with_state, with_state_mut};
use evm_db::types::keys::{make_account_key, make_storage_key};
use evm_db::types::values::{AccountVal, U256Val};
use revm::primitives::{Address, B256, U256};

mod common;

const WRAPPED_TOKEN_ADDRESS: [u8; 20] = [0x42u8; 20];
const FORWARDER_ADDRESS: [u8; 20] = [0x66u8; 20];
const TEST_FACTORY_ADDRESS: [u8; 20] = [0x55u8; 20];
const TEST_AMOUNT: u64 = 1_000_000_000_000;

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
    build_call_input_to(WRAP_PRECOMPILE_ADDRESS.into_array(), data, [0u8; 32])
}

fn build_call_input_to(to: [u8; 20], data: Vec<u8>, value: [u8; 32]) -> CallObjectInput {
    CallObjectInput {
        to: Some(to),
        from: [0x31u8; 20],
        gas_limit: Some(300_000),
        gas_price: Some(500_000_000_000),
        nonce: Some(0),
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: Some(0),
        access_list: Vec::new(),
        value,
        data,
    }
}

fn encode_icp_query_input(method: &str, arg: &[u8]) -> Vec<u8> {
    encode_icp_precompile_input(0, method, arg)
}

fn encode_icp_update_input(method: &str, arg: &[u8]) -> Vec<u8> {
    encode_icp_precompile_input(1, method, arg)
}

fn allow_icp_update_method(method: &str) {
    let target = candid::Principal::self_authenticating(b"query-target");
    with_state_mut(|state| {
        state
            .icp_update_precompile_allowlist
            .insert(precompile_allow_key(target.as_slice(), method), 1);
    });
}

fn encode_icp_precompile_input(kind: u8, method: &str, arg: &[u8]) -> Vec<u8> {
    let target = candid::Principal::self_authenticating(b"query-target");
    let target_bytes = target.as_slice();
    let mut out = Vec::new();
    out.push(1);
    out.push(kind);
    out.push(target_bytes.len() as u8);
    out.extend_from_slice(target_bytes);
    out.push(method.len() as u8);
    out.extend_from_slice(method.as_bytes());
    out.extend_from_slice(&(arg.len() as u32).to_be_bytes());
    out.extend_from_slice(arg);
    out
}

fn seed_unwrap_burn_state(caller: [u8; 20]) {
    let factory = TEST_FACTORY_ADDRESS;
    let asset = vec![0x44u8, 0x55, 0x66];
    let amount = U256::from(TEST_AMOUNT);

    let mut chain_bytes = [0u8; 32];
    chain_bytes[24..].copy_from_slice(&CHAIN_ID.to_be_bytes());
    let asset_key = keccak(
        &[
            b"kasane.wrap.v1".as_slice(),
            chain_bytes.as_slice(),
            asset.as_slice(),
        ]
        .concat(),
    );
    let factory_slot = mapping_slot(B256::from(asset_key), U256::ZERO);
    let balance_slot = address_mapping_slot(Address::new(caller), 3);
    let allowance_slot = allowance_slot(Address::new(caller), Address::new(factory));

    let mut token_word = [0u8; 32];
    token_word[12..].copy_from_slice(&WRAPPED_TOKEN_ADDRESS);

    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(factory),
            AccountVal::from_parts(1, [0u8; 32], [0x11u8; 32]),
        );
        state.accounts.insert(
            make_account_key(WRAPPED_TOKEN_ADDRESS),
            AccountVal::from_parts(1, [0u8; 32], [0x22u8; 32]),
        );
        state.storage.insert(
            make_storage_key(factory, factory_slot.to_be_bytes::<32>()),
            U256Val::new(token_word),
        );
        state.storage.insert(
            make_storage_key(WRAPPED_TOKEN_ADDRESS, U256::from(2u64).to_be_bytes::<32>()),
            U256Val::new(amount.to_be_bytes::<32>()),
        );
        state.storage.insert(
            make_storage_key(WRAPPED_TOKEN_ADDRESS, balance_slot.to_be_bytes::<32>()),
            U256Val::new(amount.to_be_bytes::<32>()),
        );
        state.storage.insert(
            make_storage_key(WRAPPED_TOKEN_ADDRESS, allowance_slot.to_be_bytes::<32>()),
            U256Val::new(amount.to_be_bytes::<32>()),
        );
    });
}

fn read_token_storage(slot: U256) -> U256 {
    with_state(|state| {
        state
            .storage
            .get(&make_storage_key(
                WRAPPED_TOKEN_ADDRESS,
                slot.to_be_bytes::<32>(),
            ))
            .map(|value| U256::from_be_bytes(value.0))
            .unwrap_or(U256::ZERO)
    })
}

fn forwarder_runtime_bytecode() -> Vec<u8> {
    forwarder_runtime_bytecode_to(WRAP_PRECOMPILE_ADDRESS.into_array())
}

fn forwarder_runtime_bytecode_to(target: [u8; 20]) -> Vec<u8> {
    let mut code = vec![0x36, 0x3d, 0x3d, 0x37, 0x3d, 0x3d, 0x36, 0x3d, 0x3d, 0x73];
    code.extend_from_slice(&target);
    code.extend_from_slice(&[0x5a, 0xf1, 0x60, 0x26, 0x57, 0x3d, 0x3d, 0xfd, 0x5b, 0x00]);
    code
}

fn relax_fee_floor_for_tests() {
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1;
        chain_state.min_gas_price = 1;
        chain_state.min_priority_fee = 1;
        state.chain_state.set(chain_state);
    });
}

fn setup_query_precompile_call_context() {
    init_stable_state();
    set_runtime_config(RuntimeConfigV1::new(
        candid::Principal::self_authenticating(b"wrap-precompile-query"),
        TEST_FACTORY_ADDRESS,
    ));
    relax_fee_floor_for_tests();
    chain::credit_balance([0x31u8; 20], 1_000_000_000_000_000_000u128).expect("fund caller");
}

fn mapping_slot(key: B256, slot: U256) -> U256 {
    let mut input = [0u8; 64];
    input[..32].copy_from_slice(key.as_slice());
    input[32..].copy_from_slice(&slot.to_be_bytes::<32>());
    U256::from_be_bytes(keccak(&input))
}

fn address_mapping_slot(key: Address, slot: u64) -> U256 {
    let mut key_bytes = [0u8; 32];
    key_bytes[12..].copy_from_slice(key.as_slice());
    mapping_slot(B256::from(key_bytes), U256::from(slot))
}

fn allowance_slot(owner: Address, spender: Address) -> U256 {
    let outer = address_mapping_slot(owner, 4);
    let mut spender_bytes = [0u8; 32];
    spender_bytes[12..].copy_from_slice(spender.as_slice());
    mapping_slot(B256::from(spender_bytes), outer)
}

fn keccak(data: &[u8]) -> [u8; 32] {
    evm_core::hash::keccak256(data)
}

#[test]
fn kasane_precompiles_eth_call_object_succeeds_in_query_path() {
    init_stable_state();
    set_runtime_config(RuntimeConfigV1::new(
        candid::Principal::self_authenticating(b"wrap-precompile-query"),
        TEST_FACTORY_ADDRESS,
    ));
    let caller = [0x31u8; 20];
    chain::credit_balance(caller, 1_000_000_000_000_000_000u128).expect("fund caller");
    seed_unwrap_burn_state(caller);
    let input = encode_unwrap_input();
    let out = chain::eth_call_object(build_call_input(input)).expect("eth_call_object");

    assert_eq!(out.status, 1);
    assert!(out.return_data.is_empty());
    assert!(out.revert_data.is_none());
}

#[test]
fn kasane_precompiles_eth_estimate_gas_succeeds_in_query_path() {
    init_stable_state();
    set_runtime_config(RuntimeConfigV1::new(
        candid::Principal::self_authenticating(b"wrap-precompile-query"),
        TEST_FACTORY_ADDRESS,
    ));
    let caller = [0x31u8; 20];
    chain::credit_balance(caller, 1_000_000_000_000_000_000u128).expect("fund caller");
    seed_unwrap_burn_state(caller);
    let input = encode_unwrap_input();
    let gas = chain::eth_estimate_gas_object(build_call_input(input)).expect("estimate gas");

    assert!(gas > 0);
    assert!(gas <= 300_000);
}

#[test]
fn kasane_precompiles_query_icp_query_precompile_async_returns_resolver_reply() {
    setup_query_precompile_call_context();
    let request_arg = vec![0x44, 0x49, 0x44, 0x4c];
    let input = encode_icp_query_input("read_state", &request_arg);
    let expected_reply = vec![0xaa, 0xbb, 0xcc];
    let mut resolver_called = false;

    let out = common::run_ready_future(chain::eth_call_object_async(
        build_call_input_to(ICP_QUERY_PRECOMPILE_ADDRESS.into_array(), input, [0u8; 32]),
        |request| {
            resolver_called = true;
            assert_eq!(request.method, "read_state");
            assert_eq!(request.arg, request_arg);
            let reply = expected_reply.clone();
            async move { Ok(reply) }
        },
    ))
    .expect("async call");

    assert!(resolver_called);
    assert_eq!(out.status, 1);
    assert_eq!(out.return_data, expected_reply);
    assert!(out.revert_data.is_none());
}

#[test]
fn kasane_precompiles_query_icp_query_precompile_async_reverts_on_resolver_error() {
    setup_query_precompile_call_context();
    let input = encode_icp_query_input("read_state", &[]);

    let out = common::run_ready_future(chain::eth_call_object_async(
        build_call_input_to(ICP_QUERY_PRECOMPILE_ADDRESS.into_array(), input, [0u8; 32]),
        |_| async { Err("ic_query.test_error".to_string()) },
    ))
    .expect("async call");

    assert_eq!(out.status, 0);
    assert!(out.return_data.is_empty());
    assert!(out.revert_data.is_none());
}

#[test]
fn kasane_precompiles_query_icp_query_precompile_requires_async_context() {
    setup_query_precompile_call_context();
    let input = encode_icp_query_input("read_state", &[]);
    let err = chain::eth_call_object(build_call_input_to(
        ICP_QUERY_PRECOMPILE_ADDRESS.into_array(),
        input,
        [0u8; 32],
    ))
    .unwrap_err();

    let ChainError::ExecFailed(Some(ExecError::ExternalQuery(request))) = err else {
        panic!("expected external query context failure");
    };
    assert_eq!(request.method, "read_state");
    assert!(request.arg.is_empty());
}

#[test]
fn kasane_precompiles_query_icp_query_precompile_async_rejects_value() {
    setup_query_precompile_call_context();
    let input = encode_icp_query_input("read_state", &[]);
    let mut value = [0u8; 32];
    value[31] = 1;
    let mut resolver_called = false;

    let out = common::run_ready_future(chain::eth_call_object_async(
        build_call_input_to(ICP_QUERY_PRECOMPILE_ADDRESS.into_array(), input, value),
        |_| {
            resolver_called = true;
            async { Ok(Vec::new()) }
        },
    ))
    .expect("async call");

    assert!(!resolver_called);
    assert_eq!(out.status, 0);
    assert!(out.return_data.is_empty());
    assert!(out.revert_data.is_none());
}

#[test]
fn wrap_precompile_burns_contract_balance_when_called_through_forwarder() {
    init_stable_state();
    set_runtime_config(RuntimeConfigV1::new(
        candid::Principal::self_authenticating(b"wrap-precompile-query"),
        TEST_FACTORY_ADDRESS,
    ));
    relax_fee_floor_for_tests();

    let caller_principal = vec![0x31u8];
    let caller = hash::derive_evm_address_from_principal(&caller_principal).expect("must derive");
    common::fund_account(caller, 1_000_000_000_000_000_000u128);
    common::install_contract(FORWARDER_ADDRESS, &forwarder_runtime_bytecode());
    seed_unwrap_burn_state(FORWARDER_ADDRESS);

    let tx_id = chain::submit_ic_tx_input(
        caller_principal,
        vec![0xa0],
        IcSyntheticTxInput {
            to: Some(FORWARDER_ADDRESS),
            value: [0u8; 32],
            gas_limit: 300_000,
            nonce: 0,
            max_fee_per_gas: 2_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            data: encode_unwrap_input(),
        },
    )
    .expect("submit");

    let produced = chain::produce_block(1).expect("produce");
    assert_eq!(produced.block.tx_ids, vec![tx_id]);

    let receipt = chain::get_receipt(&tx_id).expect("receipt");
    assert_eq!(receipt.status, 1);

    let caller_balance_slot = address_mapping_slot(Address::new(caller), 3);
    let contract_balance_slot = address_mapping_slot(Address::new(FORWARDER_ADDRESS), 3);
    let caller_allowance_slot =
        allowance_slot(Address::new(caller), Address::new(TEST_FACTORY_ADDRESS));
    let contract_allowance_slot = allowance_slot(
        Address::new(FORWARDER_ADDRESS),
        Address::new(TEST_FACTORY_ADDRESS),
    );

    assert_eq!(read_token_storage(U256::from(2u64)), U256::ZERO);
    assert_eq!(read_token_storage(caller_balance_slot), U256::ZERO);
    assert_eq!(read_token_storage(caller_allowance_slot), U256::ZERO);
    assert_eq!(read_token_storage(contract_balance_slot), U256::ZERO);
    assert_eq!(read_token_storage(contract_allowance_slot), U256::ZERO);
}

#[test]
fn icp_update_intent_precompile_emits_log_when_called_through_forwarder() {
    setup_query_precompile_call_context();
    allow_icp_update_method("write_state");
    common::install_contract(
        FORWARDER_ADDRESS,
        &forwarder_runtime_bytecode_to(ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array()),
    );

    let caller_principal = vec![0x32u8];
    let caller = hash::derive_evm_address_from_principal(&caller_principal).expect("must derive");
    common::fund_account(caller, 1_000_000_000_000_000_000u128);
    let arg = vec![0x44, 0x49, 0x44, 0x4c];
    let tx_id = chain::submit_ic_tx_input(
        caller_principal,
        vec![0xa1],
        IcSyntheticTxInput {
            to: Some(FORWARDER_ADDRESS),
            value: [0u8; 32],
            gas_limit: 300_000,
            nonce: 0,
            max_fee_per_gas: 2_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            data: encode_icp_update_input("write_state", &arg),
        },
    )
    .expect("submit");

    let produced = chain::produce_block(1).expect("produce");
    assert_eq!(produced.block.tx_ids, vec![tx_id]);

    let receipt = chain::get_receipt(&tx_id).expect("receipt");
    assert_eq!(receipt.status, 1);
    assert_eq!(receipt.logs.len(), 1);
    let log = &receipt.logs[0];
    assert_eq!(
        log.address.into_array(),
        ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array()
    );
    assert_eq!(log.topics().len(), 1);
    assert_eq!(
        log.topics()[0].0,
        hash::keccak256(b"KasaneIcpUpdateIntent(bytes)")
    );
    assert!(!log.data.data.is_empty());
}

#[test]
fn icp_update_intent_precompile_allowlist_miss_fails_without_log() {
    setup_query_precompile_call_context();
    common::install_contract(
        FORWARDER_ADDRESS,
        &forwarder_runtime_bytecode_to(ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array()),
    );

    let caller_principal = vec![0x33u8];
    let caller = hash::derive_evm_address_from_principal(&caller_principal).expect("must derive");
    common::fund_account(caller, 1_000_000_000_000_000_000u128);
    let tx_id = chain::submit_ic_tx_input(
        caller_principal,
        vec![0xa1],
        IcSyntheticTxInput {
            to: Some(FORWARDER_ADDRESS),
            value: [0u8; 32],
            gas_limit: 300_000,
            nonce: 0,
            max_fee_per_gas: 2_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            data: encode_icp_update_input("write_state", &[0x44]),
        },
    )
    .expect("submit");

    let produced = chain::produce_block(1).expect("produce");
    assert_eq!(produced.block.tx_ids, vec![tx_id]);

    let receipt = chain::get_receipt(&tx_id).expect("receipt");
    assert_eq!(receipt.status, 0);
    assert!(receipt.logs.is_empty());
}
