//! どこで: evm-core query系テスト
//! 何を: Kasane precompile 群が eth_call / estimateGas で利用可能かを固定
//! なぜ: 実送信だけ成功して query 見積もりが壊れる回帰を防ぐため

use candid::Encode;
use evm_core::chain::{self, CallObjectInput, ChainError};
use evm_core::hash;
use evm_core::kasane_precompiles::{
    configure_precompile_instruction_counter_for_test, precompile_allow_key,
    ICP_QUERY_PRECOMPILE_ADDRESS, ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS,
    NATIVE_WITHDRAW_PRECOMPILE_ADDRESS, WRAP_PRECOMPILE_ADDRESS,
};
use evm_core::revm_exec::{configure_instruction_budget_tripped_for_test, ExecError};
use evm_core::tx_decode::IcSyntheticTxInput;
use evm_db::chain_data::{
    constants::CHAIN_ID, constants::MAX_RETURN_DATA, Head, IcpUpdateDispatchRequest,
    IcpUpdateRequestStatus, RuntimeConfigV1, TxId, TxKind, MAX_ICP_UPDATE_REQUESTS,
};
use evm_db::stable_state::{
    current_evm_state_epoch, init_stable_state, set_runtime_config, with_state, with_state_mut,
};
use evm_db::types::keys::{make_account_key, make_storage_key};
use evm_db::types::values::{AccountVal, U256Val};
use revm::primitives::{Address, B256, U256};

mod common;

const WRAPPED_TOKEN_ADDRESS: [u8; 20] = [0x42u8; 20];
const FORWARDER_ADDRESS: [u8; 20] = [0x66u8; 20];
const REVERTING_FORWARDER_ADDRESS: [u8; 20] = [0x67u8; 20];
const UPDATE_RETRY_ADDRESS: [u8; 20] = [0x68u8; 20];
const DOUBLE_QUERY_ADDRESS: [u8; 20] = [0x72u8; 20];
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

fn encode_native_withdraw_input() -> Vec<u8> {
    let recipient = vec![0x77u8, 0x88, 0x99];
    let mut principal = vec![0u8; 30];
    principal[0] = recipient.len() as u8;
    principal[1..1 + recipient.len()].copy_from_slice(&recipient);

    let mut out = Vec::with_capacity(31);
    out.push(1);
    out.extend_from_slice(&principal);
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

fn query_target() -> candid::Principal {
    candid::Principal::self_authenticating(b"query-target")
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

fn test_icp_update_request(request_id: TxId) -> IcpUpdateDispatchRequest {
    IcpUpdateDispatchRequest {
        target: query_target().as_slice().to_vec(),
        method: "write_state".to_string(),
        arg: vec![0x44],
        request_id,
        tx_id: request_id,
        block_number: 1,
        tx_index: 0,
        log_index: 0,
        tx_kind: TxKind::IcSynthetic,
        evm_sender: [0x31u8; 20],
        ic_caller: None,
        status: IcpUpdateRequestStatus::Queued,
        reply: None,
        error_code: None,
        updated_at: 0,
        call_started_at_time: 0,
    }
}

fn seed_icp_update_requests(count: usize) {
    seed_icp_update_requests_with_status(count, IcpUpdateRequestStatus::Queued);
}

fn seed_icp_update_requests_with_status(count: usize, status: IcpUpdateRequestStatus) {
    with_state_mut(|state| {
        for idx in 0..count {
            let mut raw = [0x91u8; 32];
            let idx_u64 = u64::try_from(idx).expect("test index fits u64");
            raw[24..32].copy_from_slice(&idx_u64.to_be_bytes());
            let request_id = TxId(raw);
            let mut req = test_icp_update_request(request_id);
            req.status = status;
            req.updated_at = idx_u64;
            state.icp_update_requests.insert(request_id, req);
        }
    });
}

fn encode_icp_precompile_input(kind: u8, method: &str, arg: &[u8]) -> Vec<u8> {
    let target = query_target();
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

fn query_precompile_allow_key(target: candid::Principal, method: &str) -> Vec<u8> {
    let target_bytes = target.as_slice();
    let mut out = Vec::with_capacity(1 + target_bytes.len() + method.len());
    out.push(target_bytes.len() as u8);
    out.extend_from_slice(target_bytes);
    out.extend_from_slice(method.as_bytes());
    out
}

fn expect_snapshot_changed(result: Result<chain::CallObjectResult, ChainError>) {
    let Err(ChainError::ExecFailed(Some(ExecError::SnapshotChanged))) = result else {
        panic!("expected snapshot changed, got {result:?}");
    };
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

fn reverting_forwarder_runtime_bytecode_to(target: [u8; 20]) -> Vec<u8> {
    let mut code = vec![0x36, 0x3d, 0x3d, 0x37, 0x3d, 0x3d, 0x36, 0x3d, 0x3d, 0x73];
    code.extend_from_slice(&target);
    code.extend_from_slice(&[0x5a, 0xf1, 0x50, 0x3d, 0x3d, 0xfd]);
    code
}

fn update_retry_runtime_bytecode(reverting_forwarder: [u8; 20], target: [u8; 20]) -> Vec<u8> {
    let mut code = vec![0x36, 0x3d, 0x3d, 0x37];
    code.extend_from_slice(&[0x3d, 0x3d, 0x36, 0x3d, 0x3d, 0x73]);
    code.extend_from_slice(&reverting_forwarder);
    code.extend_from_slice(&[0x5a, 0xf1, 0x50, 0x3d, 0x3d, 0x36, 0x3d, 0x3d, 0x73]);
    code.extend_from_slice(&target);
    code.extend_from_slice(&[0x5a, 0xf1, 0x60]);
    let jumpdest_offset = code.len();
    code.push(0x00);
    code.extend_from_slice(&[0x57, 0x3d, 0x3d, 0xfd]);
    let jumpdest = u8::try_from(code.len()).expect("test bytecode jumpdest fits u8");
    code[jumpdest_offset] = jumpdest;
    code.extend_from_slice(&[0x5b, 0x00]);
    code
}

fn double_icp_query_runtime_bytecode() -> Vec<u8> {
    let mut code = vec![0x36, 0x3d, 0x3d, 0x37];
    for is_first in [true, false] {
        code.extend_from_slice(&[0x3d, 0x3d, 0x36, 0x3d, 0x3d, 0x73]);
        code.extend_from_slice(ICP_QUERY_PRECOMPILE_ADDRESS.as_slice());
        code.extend_from_slice(&[0x5a, 0xf1]);
        if is_first {
            code.push(0x50);
        }
    }
    code.push(0x60);
    let jumpdest_offset = code.len();
    code.push(0x00);
    code.extend_from_slice(&[0x57, 0x3d, 0x3d, 0xfd]);
    let jumpdest = u8::try_from(code.len()).expect("test bytecode jumpdest fits u8");
    code[jumpdest_offset] = jumpdest;
    code.extend_from_slice(&[0x5b, 0x00]);
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

struct PrecompileInstructionCounterGuard;

impl PrecompileInstructionCounterGuard {
    fn configure(start: u64, step: u64) -> Self {
        configure_precompile_instruction_counter_for_test(start, step);
        Self
    }
}

impl Drop for PrecompileInstructionCounterGuard {
    fn drop(&mut self) {
        configure_precompile_instruction_counter_for_test(0, 0);
    }
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
fn kasane_precompiles_query_icp_query_precompile_is_disabled_in_plain_eth_call() {
    setup_query_precompile_call_context();
    let input = encode_icp_query_input("read_state", &[]);
    let out = chain::eth_call_object(build_call_input_to(
        ICP_QUERY_PRECOMPILE_ADDRESS.into_array(),
        input,
        [0u8; 32],
    ))
    .expect("eth_call_object");

    assert_eq!(out.status, 0);
    assert!(out.return_data.is_empty());
    assert!(out.revert_data.is_none());
}

#[test]
fn wrap_precompile_query_icp_query_precompile_reverts_in_block_tx_without_external_query() {
    setup_query_precompile_call_context();
    let caller_principal = vec![0x41u8];
    let caller = hash::derive_evm_address_from_principal(&caller_principal).expect("must derive");
    common::fund_account(caller, 1_000_000_000_000_000_000u128);

    let tx_id = chain::submit_ic_tx_input(
        caller_principal,
        vec![0xa0],
        IcSyntheticTxInput {
            to: Some(ICP_QUERY_PRECOMPILE_ADDRESS.into_array()),
            value: [0u8; 32],
            gas_limit: 300_000,
            nonce: 0,
            max_fee_per_gas: 2_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            data: encode_icp_query_input("read_state", &[]),
        },
    )
    .expect("submit");

    let produced = chain::produce_block(1).expect("produce");
    assert_eq!(produced.block.tx_ids, vec![tx_id]);
    let receipt = chain::get_receipt(&tx_id).expect("receipt");
    assert_eq!(receipt.status, 0);
}

#[test]
fn wrap_precompile_query_async_rejects_wrap_precompile_access() {
    setup_query_precompile_call_context();
    let caller = [0x31u8; 20];
    seed_unwrap_burn_state(caller);
    let mut resolver_called = false;

    let out = common::run_ready_future(chain::eth_call_object_async(
        build_call_input(encode_unwrap_input()),
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
fn wrap_precompile_query_async_rejects_native_withdraw_access() {
    setup_query_precompile_call_context();
    let mut resolver_called = false;

    let out = common::run_ready_future(chain::eth_call_object_async(
        build_call_input_to(
            NATIVE_WITHDRAW_PRECOMPILE_ADDRESS.into_array(),
            encode_native_withdraw_input(),
            [0u8; 32],
        ),
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
fn wrap_precompile_query_call_limit_reverts_second_call() {
    setup_query_precompile_call_context();
    common::install_contract(DOUBLE_QUERY_ADDRESS, &double_icp_query_runtime_bytecode());
    let input = encode_icp_query_input("read_state", &[]);
    let mut resolver_calls = 0u32;

    let out = common::run_ready_future(chain::eth_call_object_async(
        build_call_input_to(DOUBLE_QUERY_ADDRESS, input, [0u8; 32]),
        |_| {
            resolver_calls = resolver_calls.saturating_add(1);
            async { Ok(vec![0xaa]) }
        },
    ))
    .expect("async call");

    assert_eq!(resolver_calls, 1);
    assert_eq!(out.status, 0);
    assert!(out.return_data.is_empty());
    assert!(out.revert_data.is_none());
}

#[test]
fn wrap_precompile_query_budget_exceeded_before_precompile_does_not_call_resolver() {
    setup_query_precompile_call_context();
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.query_instruction_soft_limit = 1;
        state.chain_state.set(chain_state);
    });
    configure_instruction_budget_tripped_for_test(true);
    let input = encode_icp_query_input("read_state", &[]);
    let mut resolver_called = false;

    let result = common::run_ready_future(chain::eth_call_object_async(
        build_call_input_to(ICP_QUERY_PRECOMPILE_ADDRESS.into_array(), input, [0u8; 32]),
        |_| {
            resolver_called = true;
            async { Ok(Vec::new()) }
        },
    ));
    configure_instruction_budget_tripped_for_test(false);

    assert!(!resolver_called);
    assert!(matches!(
        result,
        Err(ChainError::ExecFailed(Some(
            ExecError::InstructionBudgetExceeded
        )))
    ));
}

#[test]
fn wrap_precompile_query_snapshot_guard_rejects_head_change() {
    setup_query_precompile_call_context();
    let input = encode_icp_query_input("read_state", &[]);

    let result = common::run_ready_future(chain::eth_call_object_async(
        build_call_input_to(ICP_QUERY_PRECOMPILE_ADDRESS.into_array(), input, [0u8; 32]),
        |_| {
            with_state_mut(|state| {
                let head = *state.head.get();
                state.head.set(Head {
                    number: head.number.saturating_add(1),
                    block_hash: [0x77u8; 32],
                    timestamp: head.timestamp.saturating_add(1),
                });
            });
            async { Ok(Vec::new()) }
        },
    ));

    expect_snapshot_changed(result);
}

#[test]
fn wrap_precompile_query_snapshot_guard_rejects_chain_state_change() {
    setup_query_precompile_call_context();
    let input = encode_icp_query_input("read_state", &[]);

    let result = common::run_ready_future(chain::eth_call_object_async(
        build_call_input_to(ICP_QUERY_PRECOMPILE_ADDRESS.into_array(), input, [0u8; 32]),
        |_| {
            with_state_mut(|state| {
                let mut chain_state = *state.chain_state.get();
                chain_state.base_fee = chain_state.base_fee.saturating_add(1);
                state.chain_state.set(chain_state);
            });
            async { Ok(Vec::new()) }
        },
    ));

    expect_snapshot_changed(result);
}

#[test]
fn wrap_precompile_query_snapshot_guard_rejects_allowlist_change() {
    setup_query_precompile_call_context();
    let input = encode_icp_query_input("read_state", &[]);

    let result = common::run_ready_future(chain::eth_call_object_async(
        build_call_input_to(ICP_QUERY_PRECOMPILE_ADDRESS.into_array(), input, [0u8; 32]),
        |_| {
            with_state_mut(|state| {
                state
                    .query_precompile_allowlist
                    .insert(query_precompile_allow_key(query_target(), "read_state"), 1);
            });
            async { Ok(Vec::new()) }
        },
    ));

    expect_snapshot_changed(result);
}

#[test]
fn wrap_precompile_query_snapshot_guard_rejects_runtime_config_change() {
    setup_query_precompile_call_context();
    let input = encode_icp_query_input("read_state", &[]);

    let result = common::run_ready_future(chain::eth_call_object_async(
        build_call_input_to(ICP_QUERY_PRECOMPILE_ADDRESS.into_array(), input, [0u8; 32]),
        |_| {
            set_runtime_config(RuntimeConfigV1::new(
                candid::Principal::self_authenticating(b"changed-runtime-config"),
                [0x99u8; 20],
            ));
            async { Ok(Vec::new()) }
        },
    ));

    expect_snapshot_changed(result);
}

#[test]
fn wrap_precompile_query_snapshot_guard_rejects_credit_balance_epoch_change() {
    setup_query_precompile_call_context();
    let input = encode_icp_query_input("read_state", &[]);

    let result = common::run_ready_future(chain::eth_call_object_async(
        build_call_input_to(ICP_QUERY_PRECOMPILE_ADDRESS.into_array(), input, [0u8; 32]),
        |_| {
            chain::credit_balance([0x82u8; 20], 1).expect("credit");
            async { Ok(Vec::new()) }
        },
    ));

    expect_snapshot_changed(result);
}

#[test]
fn wrap_precompile_query_snapshot_guard_rejects_native_credit_epoch_change() {
    setup_query_precompile_call_context();
    let input = encode_icp_query_input("read_state", &[]);

    let result = common::run_ready_future(chain::eth_call_object_async(
        build_call_input_to(ICP_QUERY_PRECOMPILE_ADDRESS.into_array(), input, [0u8; 32]),
        |_| {
            let mut amount = [0u8; 32];
            amount[31] = 1;
            chain::credit_native_deposit([0x83u8; 32], [0x84u8; 20], amount)
                .expect("native credit");
            async { Ok(Vec::new()) }
        },
    ));

    expect_snapshot_changed(result);
}

#[test]
fn wrap_precompile_query_native_credit_duplicate_does_not_bump_epoch() {
    setup_query_precompile_call_context();
    let mut amount = [0u8; 32];
    amount[31] = 1;
    chain::credit_native_deposit([0x85u8; 32], [0x86u8; 20], amount).expect("native credit");
    let after_first = current_evm_state_epoch();

    chain::credit_native_deposit([0x85u8; 32], [0x86u8; 20], amount)
        .expect("duplicate native credit");

    assert_eq!(current_evm_state_epoch(), after_first);
}

#[test]
fn wrap_precompile_query_raw_candid_replies_are_not_reencoded() {
    let replies: Vec<Vec<u8>> = vec![
        b"DIDL\0\0".to_vec(),
        candid::Encode!(&candid::Nat::from(42u64), &"ok").expect("encode tuple"),
        vec![0xff, 0x00, 0x44],
    ];

    for reply in replies {
        setup_query_precompile_call_context();
        let input = encode_icp_query_input("read_state", &[]);
        let expected = reply.clone();

        let out = common::run_ready_future(chain::eth_call_object_async(
            build_call_input_to(ICP_QUERY_PRECOMPILE_ADDRESS.into_array(), input, [0u8; 32]),
            |_| {
                let reply = reply.clone();
                async move { Ok(reply) }
            },
        ))
        .expect("async call");

        assert_eq!(out.status, 1);
        assert_eq!(out.return_data, expected);
        assert!(out.revert_data.is_none());
    }
}

#[test]
fn wrap_precompile_query_large_raw_reply_is_rejected() {
    setup_query_precompile_call_context();
    let input = encode_icp_query_input("read_state", &[]);
    let large_reply = vec![0u8; MAX_RETURN_DATA + 1];

    let result = common::run_ready_future(chain::eth_call_object_async(
        build_call_input_to(ICP_QUERY_PRECOMPILE_ADDRESS.into_array(), input, [0u8; 32]),
        |_| {
            let large_reply = large_reply.clone();
            async move { Ok(large_reply) }
        },
    ));

    let out = result.expect("large reply should become EVM-level failure");
    assert_eq!(out.status, 0);
    assert!(out.return_data.is_empty());
    assert!(out.revert_data.is_none());
}

#[test]
fn wrap_precompile_query_sys_unknown_resolver_error_is_stable_revert() {
    setup_query_precompile_call_context();
    let input = encode_icp_query_input("read_state", &[]);

    let out = common::run_ready_future(chain::eth_call_object_async(
        build_call_input_to(ICP_QUERY_PRECOMPILE_ADDRESS.into_array(), input, [0u8; 32]),
        |_| async { Err("ic_query.call_failed:SysUnknown".to_string()) },
    ))
    .expect("async call");

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
fn icp_update_intent_capacity_is_reserved_within_block() {
    setup_query_precompile_call_context();
    allow_icp_update_method("write_state");
    seed_icp_update_requests(MAX_ICP_UPDATE_REQUESTS - 1);
    common::install_contract(
        FORWARDER_ADDRESS,
        &forwarder_runtime_bytecode_to(ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array()),
    );

    let first_principal = vec![0x34u8];
    let second_principal = vec![0x36u8];
    let first_caller =
        hash::derive_evm_address_from_principal(&first_principal).expect("must derive");
    let second_caller =
        hash::derive_evm_address_from_principal(&second_principal).expect("must derive");
    common::fund_account(first_caller, 1_000_000_000_000_000_000u128);
    common::fund_account(second_caller, 1_000_000_000_000_000_000u128);
    let first = chain::submit_ic_tx_input(
        first_principal,
        vec![0xa2],
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
    .expect("submit first");
    let second = chain::submit_ic_tx_input(
        second_principal,
        vec![0xa3],
        IcSyntheticTxInput {
            to: Some(FORWARDER_ADDRESS),
            value: [0u8; 32],
            gas_limit: 300_000,
            nonce: 0,
            max_fee_per_gas: 2_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            data: encode_icp_update_input("write_state", &[0x45]),
        },
    )
    .expect("submit second");

    let produced = chain::produce_block(2).expect("produce");
    assert_eq!(produced.block.tx_ids, vec![first, second]);

    let first_receipt = chain::get_receipt(&first).expect("first receipt");
    assert_eq!(first_receipt.status, 1);
    assert_eq!(first_receipt.logs.len(), 1);

    let second_receipt = chain::get_receipt(&second).expect("second receipt");
    assert_eq!(second_receipt.status, 0);
    assert!(second_receipt.logs.is_empty());
}

#[test]
fn icp_update_intent_full_capacity_reverts_without_stopping_block() {
    setup_query_precompile_call_context();
    allow_icp_update_method("write_state");
    seed_icp_update_requests(MAX_ICP_UPDATE_REQUESTS);
    common::install_contract(
        FORWARDER_ADDRESS,
        &forwarder_runtime_bytecode_to(ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array()),
    );

    let caller_principal = vec![0x35u8];
    let caller = hash::derive_evm_address_from_principal(&caller_principal).expect("must derive");
    common::fund_account(caller, 1_000_000_000_000_000_000u128);
    let tx_id = chain::submit_ic_tx_input(
        caller_principal,
        vec![0xa4],
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

#[test]
fn icp_update_intent_dispatching_capacity_reverts_without_stopping_block() {
    setup_query_precompile_call_context();
    allow_icp_update_method("write_state");
    seed_icp_update_requests_with_status(
        MAX_ICP_UPDATE_REQUESTS,
        IcpUpdateRequestStatus::Dispatching,
    );
    common::install_contract(
        FORWARDER_ADDRESS,
        &forwarder_runtime_bytecode_to(ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array()),
    );

    let caller_principal = vec![0x3bu8];
    let caller = hash::derive_evm_address_from_principal(&caller_principal).expect("must derive");
    common::fund_account(caller, 1_000_000_000_000_000_000u128);
    let tx_id = chain::submit_ic_tx_input(
        caller_principal,
        vec![0xa8],
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

#[test]
fn icp_update_intent_terminal_history_does_not_consume_capacity() {
    for (offset, status) in [
        (0u8, IcpUpdateRequestStatus::Dispatched),
        (1u8, IcpUpdateRequestStatus::DispatchFailed),
        (2u8, IcpUpdateRequestStatus::DispatchUncertain),
    ] {
        setup_query_precompile_call_context();
        allow_icp_update_method("write_state");
        seed_icp_update_requests_with_status(MAX_ICP_UPDATE_REQUESTS, status);
        common::install_contract(
            FORWARDER_ADDRESS,
            &forwarder_runtime_bytecode_to(ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array()),
        );

        let caller_principal = vec![0x38u8 + offset];
        let caller =
            hash::derive_evm_address_from_principal(&caller_principal).expect("must derive");
        common::fund_account(caller, 1_000_000_000_000_000_000u128);
        let tx_id = chain::submit_ic_tx_input(
            caller_principal,
            vec![0xa6 + offset],
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
        assert_eq!(receipt.status, 1);
        assert_eq!(receipt.logs.len(), 1);
    }
}

#[test]
fn icp_update_intent_extra_gas_oog_does_not_emit_log_or_dispatch_request() {
    setup_query_precompile_call_context();
    allow_icp_update_method("write_state");
    common::install_contract(
        FORWARDER_ADDRESS,
        &forwarder_runtime_bytecode_to(ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array()),
    );

    let caller_principal = vec![0x3cu8];
    let caller = hash::derive_evm_address_from_principal(&caller_principal).expect("must derive");
    common::fund_account(caller, 1_000_000_000_000_000_000u128);
    let tx_id = chain::submit_ic_tx_input(
        caller_principal,
        vec![0xa9],
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

    let _counter_guard = PrecompileInstructionCounterGuard::configure(0, 1_000_000_000);
    let produced = chain::produce_block(1).expect("produce");
    assert_eq!(produced.block.tx_ids, vec![tx_id]);

    let receipt = chain::get_receipt(&tx_id).expect("receipt");
    assert_eq!(receipt.status, 0);
    assert!(receipt.logs.is_empty());
    with_state(|state| {
        assert_eq!(state.icp_update_requests.len(), 0);
    });
}

#[test]
fn icp_update_intent_reverted_subcall_does_not_consume_capacity() {
    setup_query_precompile_call_context();
    allow_icp_update_method("write_state");
    seed_icp_update_requests(MAX_ICP_UPDATE_REQUESTS - 1);
    common::install_contract(
        REVERTING_FORWARDER_ADDRESS,
        &reverting_forwarder_runtime_bytecode_to(ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array()),
    );
    common::install_contract(
        UPDATE_RETRY_ADDRESS,
        &update_retry_runtime_bytecode(
            REVERTING_FORWARDER_ADDRESS,
            ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array(),
        ),
    );

    let caller_principal = vec![0x37u8];
    let caller = hash::derive_evm_address_from_principal(&caller_principal).expect("must derive");
    common::fund_account(caller, 1_000_000_000_000_000_000u128);
    let tx_id = chain::submit_ic_tx_input(
        caller_principal,
        vec![0xa5],
        IcSyntheticTxInput {
            to: Some(UPDATE_RETRY_ADDRESS),
            value: [0u8; 32],
            gas_limit: 500_000,
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
    assert_eq!(receipt.status, 1);
    assert_eq!(receipt.logs.len(), 1);
}

#[test]
fn icp_update_intent_capacity_does_not_limit_eth_call() {
    setup_query_precompile_call_context();
    allow_icp_update_method("write_state");
    seed_icp_update_requests(MAX_ICP_UPDATE_REQUESTS);
    common::install_contract(
        FORWARDER_ADDRESS,
        &forwarder_runtime_bytecode_to(ICP_UPDATE_INTENT_PRECOMPILE_ADDRESS.into_array()),
    );

    let out = chain::eth_call_object(build_call_input_to(
        FORWARDER_ADDRESS,
        encode_icp_update_input("write_state", &[0x44]),
        [0u8; 32],
    ))
    .expect("eth_call");

    assert_eq!(out.status, 1);
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
