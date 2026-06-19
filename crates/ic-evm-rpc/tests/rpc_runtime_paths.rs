//! どこで: ic-evm-rpc の統合テスト
//! 何を: 実運用で使うRPC経路のエラーマッピングと基本挙動を検証
//! なぜ: wrapper側テストと実運用実装の乖離を防ぐため

use alloy_consensus::{SignableTransaction, TxEip1559};
use alloy_eips::eip2718::Encodable2718;
use alloy_eips::eip2930::AccessList;
use alloy_primitives::{Address, Bytes, TxKind as EthTxKind, U256 as AlloyU256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use evm_core::chain::{self, TxIn};
use evm_core::hash;
use evm_core::kasane_precompiles::ICP_QUERY_PRECOMPILE_ADDRESS;
use evm_core::tx_decode::IcSyntheticTxInput;
use evm_db::chain_data::constants::{CHAIN_ID, MAX_TX_SIZE};
use evm_db::chain_data::receipt::log_entry_from_parts;
use evm_db::chain_data::runtime_defaults::{DEFAULT_BASE_FEE, DEFAULT_MIN_FEE_FLOOR};
use evm_db::chain_data::{
    BlockData, Head, ReceiptLike, SenderKey, StoredTxBytes, TxId, TxIndexEntry, TxKind,
};
use evm_db::stable_state::{init_stable_state, with_state_mut};
use evm_db::types::keys::{make_account_key, make_code_key, make_storage_key};
use evm_db::types::values::{AccountVal, CodeVal, U256Val};
use evm_db::Storable;
use ic_evm_rpc::{
    rpc_eth_call_object, rpc_eth_call_object_at, rpc_eth_call_object_at_async, rpc_eth_call_rawtx,
    rpc_eth_estimate_gas_object, rpc_eth_estimate_gas_object_at, rpc_eth_fee_history,
    rpc_eth_gas_price, rpc_eth_get_balance, rpc_eth_get_block_by_number_with_status,
    rpc_eth_get_block_number_by_hash, rpc_eth_get_code, rpc_eth_get_logs_paged,
    rpc_eth_get_storage_at, rpc_eth_get_transaction_by_eth_hash, rpc_eth_get_transaction_count_at,
    rpc_eth_get_transaction_receipt_by_eth_hash,
    rpc_eth_get_transaction_receipt_with_status_by_eth_hash,
    rpc_eth_get_transaction_receipt_with_status_by_tx_id, rpc_eth_history_window,
    rpc_eth_max_priority_fee_per_gas, rpc_eth_send_raw_transaction, submit_tx_in_with_code,
};
use ic_evm_rpc_types::{
    EthLogFilterView, RpcBlockLookupView, RpcBlockTagView, RpcCallObjectView, RpcReceiptLookupView,
};
use std::future::Future;
use std::pin::pin;
use std::sync::{Mutex, OnceLock};
use std::task::{Context, Poll, Waker};

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn run_ready_future<F>(future: F) -> F::Output
where
    F: Future,
{
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = pin!(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("test future must complete without suspension"),
    }
}

fn encode_icp_query_input(method: &str, arg: &[u8]) -> Vec<u8> {
    let target = candid::Principal::self_authenticating(b"rpc-query-target");
    let target_bytes = target.as_slice();
    let mut out = Vec::new();
    out.push(1);
    out.push(0);
    out.push(target_bytes.len() as u8);
    out.extend_from_slice(target_bytes);
    out.push(method.len() as u8);
    out.extend_from_slice(method.as_bytes());
    out.extend_from_slice(&(arg.len() as u32).to_be_bytes());
    out.extend_from_slice(arg);
    out
}

fn icp_query_call_object(from: [u8; 20], method: &str, arg: &[u8]) -> RpcCallObjectView {
    RpcCallObjectView {
        to: Some(ICP_QUERY_PRECOMPILE_ADDRESS.as_slice().to_vec()),
        from: Some(from.to_vec()),
        gas: Some(300_000),
        gas_price: Some(u128::from(DEFAULT_BASE_FEE + DEFAULT_MIN_FEE_FLOOR)),
        nonce: Some(0),
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: Some(0),
        access_list: None,
        value: Some(vec![0u8; 32]),
        data: Some(encode_icp_query_input(method, arg)),
    }
}

fn store_fee_sample_block(max_fee_per_gas: u128, max_priority_fee_per_gas: u128) {
    let caller_principal = vec![0x11];
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 0;
        chain_state.min_gas_price = 0;
        state.chain_state.set(chain_state);
    });
    let caller = hash::derive_evm_address_from_principal(&caller_principal).expect("must derive");
    chain::credit_balance(caller, 1_000_000_000_000_000_000u128).expect("fund caller");
    let tx = IcSyntheticTxInput {
        to: Some([0x10; 20]),
        value: [0u8; 32],
        gas_limit: 50_000,
        nonce: 0,
        max_fee_per_gas,
        max_priority_fee_per_gas,
        data: Vec::new(),
    };
    let tx_id = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal,
        canister_id: vec![0x22],
        tx,
    })
    .expect("submit tx");
    let outcome = chain::produce_block(1).expect("produce block");
    assert_eq!(outcome.block.tx_ids, vec![tx_id]);
}

fn test_signer() -> PrivateKeySigner {
    "0x59c6995e998f97a5a0044966f094538e0d7f4f4e4d5d8dd6a8c4f9d5f8b1e8a1"
        .parse()
        .expect("signer")
}

fn build_eth_signed_1559(
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) -> Vec<u8> {
    let signer = test_signer();
    let tx = TxEip1559 {
        chain_id: CHAIN_ID,
        nonce,
        gas_limit: 50_000,
        max_fee_per_gas,
        max_priority_fee_per_gas,
        to: EthTxKind::Call(Address::from([0x21u8; 20])),
        value: AlloyU256::ZERO,
        access_list: AccessList::default(),
        input: Bytes::new(),
    };
    let hash = tx.signature_hash();
    let signature = signer.sign_hash_sync(&hash).expect("sign");
    tx.into_signed(signature).encoded_2718()
}

fn fund_eth_signer() {
    let signer = test_signer();
    chain::credit_balance(signer.address().into_array(), 1_000_000_000_000_000_000u128)
        .expect("fund signer");
}

fn store_eth_signed_fee_sample_block(
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) {
    fund_eth_signer();
    let raw = build_eth_signed_1559(nonce, max_fee_per_gas, max_priority_fee_per_gas);
    let tx_id = chain::submit_tx(TxKind::EthSigned, raw, vec![0x89]).expect("submit eth tx");
    let outcome = chain::produce_block(1).expect("produce block");
    assert_eq!(outcome.block.tx_ids, vec![tx_id]);
}

#[test]
fn rpc_eth_get_balance_rejects_invalid_address_length() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let err = rpc_eth_get_balance(vec![0u8; 19], RpcBlockTagView::Latest)
        .expect_err("invalid address should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "address must be 20 bytes");
    let err = rpc_eth_get_balance(vec![0u8; 32], RpcBlockTagView::Latest)
        .expect_err("bytes32-like address should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(
        err,
        ic_evm_rpc_types::RpcErrorView {
            code: 1001,
            message: "address must be 20 bytes (got 32; this looks like bytes32-encoded principal)"
                .to_string(),
            error_prefix: Some("invalid.address".to_string()),
        }
    );
}

#[test]
fn rpc_eth_get_balance_returns_zero_for_unknown_account() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let out =
        rpc_eth_get_balance(vec![0u8; 20], RpcBlockTagView::Latest).expect("query should succeed");
    assert_eq!(out, vec![0u8; 32]);
}

#[test]
fn rpc_eth_get_code_rejects_invalid_address_length() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let err = rpc_eth_get_code(vec![0u8; 21], RpcBlockTagView::Latest)
        .expect_err("invalid address should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "address must be 20 bytes");
}

#[test]
fn rpc_eth_get_code_returns_empty_for_unknown_account() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let out =
        rpc_eth_get_code(vec![0u8; 20], RpcBlockTagView::Latest).expect("query should succeed");
    assert!(out.is_empty());
}

#[test]
fn rpc_eth_get_storage_at_rejects_invalid_lengths() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let err = rpc_eth_get_storage_at(vec![0u8; 19], vec![0u8; 32], RpcBlockTagView::Latest)
        .expect_err("invalid address should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "address must be 20 bytes");
    let err = rpc_eth_get_storage_at(vec![0u8; 20], vec![0u8; 31], RpcBlockTagView::Latest)
        .expect_err("invalid slot should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "slot must be 32 bytes");
}

#[test]
fn rpc_eth_get_storage_at_returns_zero_and_reads_existing_value() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    let missing = rpc_eth_get_storage_at(vec![0u8; 20], vec![0u8; 32], RpcBlockTagView::Latest)
        .expect("query should succeed");
    assert_eq!(missing, vec![0u8; 32]);

    let addr = [0x11u8; 20];
    let slot = [0x22u8; 32];
    evm_db::stable_state::with_state_mut(|state| {
        state
            .storage
            .insert(make_storage_key(addr, slot), U256Val([0x33u8; 32]));
    });
    let out = rpc_eth_get_storage_at(addr.to_vec(), slot.to_vec(), RpcBlockTagView::Latest)
        .expect("query should succeed");
    assert_eq!(out, vec![0x33u8; 32]);
}

#[test]
fn rpc_eth_call_object_and_estimate_gas_work() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let from = [0x77u8; 20];
    evm_db::stable_state::with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(from),
            AccountVal::from_parts(0, [0xffu8; 32], [0u8; 32]),
        );
    });
    let call = RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: Some(from.to_vec()),
        gas: Some(30_000),
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: Some(vec![0u8; 32]),
        data: Some(Vec::new()),
    };
    let out = rpc_eth_call_object(call.clone()).expect("call object should succeed");
    assert_eq!(out.status, 1);
    assert!(out.gas_used > 0);
    assert!(out.revert_data.is_none());

    let gas = rpc_eth_estimate_gas_object(call).expect("estimate should succeed");
    assert!(gas > 0);
}

#[test]
fn rpc_eth_estimate_gas_object_returns_minimum_successful_gas_limit() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    let from = [0x55u8; 20];
    let to = [0x66u8; 20];
    let code = vec![
        0x5a, 0x62, 0x02, 0x49, 0xf0, 0x11, 0x60, 0x13, 0x57, 0x60, 0x01, 0x60, 0x00, 0x52, 0x60,
        0x20, 0x60, 0x00, 0xf3, 0x5b, 0x60, 0x00, 0x60, 0x00, 0xfd,
    ];
    let code_hash = hash::keccak256(&code);
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(from),
            AccountVal::from_parts(0, [0xffu8; 32], [0u8; 32]),
        );
        state.accounts.insert(
            make_account_key(to),
            AccountVal::from_parts(0, [0u8; 32], code_hash),
        );
        state
            .codes
            .insert(make_code_key(code_hash), CodeVal(code.clone()));
    });

    let call = RpcCallObjectView {
        to: Some(to.to_vec()),
        from: Some(from.to_vec()),
        gas: None,
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: Some(vec![0u8; 32]),
        data: Some(Vec::new()),
    };

    let estimate = rpc_eth_estimate_gas_object(call.clone()).expect("estimate should succeed");
    assert!(estimate >= 150_000);

    let fail = rpc_eth_call_object(RpcCallObjectView {
        gas: Some(estimate.saturating_sub(1)),
        ..call.clone()
    })
    .expect("call with insufficient gas should execute");
    assert_eq!(fail.status, 0);

    let success = rpc_eth_call_object(RpcCallObjectView {
        gas: Some(estimate),
        ..call
    })
    .expect("call with estimated gas should execute");
    assert_eq!(success.status, 1);
    assert!(success.gas_used < estimate);
}

#[test]
fn rpc_eth_txcount_at_respects_latest_and_pending_semantics() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let sender = [0x42u8; 20];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(sender),
            AccountVal::from_parts(3, [0u8; 32], [0u8; 32]),
        );
        state
            .sender_expected_nonce
            .insert(SenderKey::new(sender), 7);
        state.head.set(Head {
            number: 2,
            block_hash: [0x22u8; 32],
            timestamp: 1_700_000_002,
        });
    });
    let latest = rpc_eth_get_transaction_count_at(sender.to_vec(), RpcBlockTagView::Latest)
        .expect("latest nonce");
    assert_eq!(latest, 3);
    let by_number = rpc_eth_get_transaction_count_at(sender.to_vec(), RpcBlockTagView::Number(2))
        .expect("head-number nonce should be available");
    assert_eq!(by_number, latest);
    let past = rpc_eth_get_transaction_count_at(sender.to_vec(), RpcBlockTagView::Number(1))
        .expect_err("historical nonce should be unavailable for in-window number");
    assert_eq!(past.code, 2001);
    assert!(past.message.starts_with("exec.state.unavailable"));
    let out_of_window =
        rpc_eth_get_transaction_count_at(sender.to_vec(), RpcBlockTagView::Number(3))
            .expect_err("out-of-window number should fail");
    assert_eq!(out_of_window.code, 1001);
    assert!(out_of_window
        .message
        .starts_with("invalid.block_range.out_of_window"));
    let earliest = rpc_eth_get_transaction_count_at(sender.to_vec(), RpcBlockTagView::Earliest)
        .expect_err("historical nonce should be unavailable for earliest");
    assert_eq!(earliest.code, 2001);
    assert!(earliest.message.starts_with("exec.state.unavailable"));
    let pending = rpc_eth_get_transaction_count_at(sender.to_vec(), RpcBlockTagView::Pending)
        .expect("pending nonce");
    assert_eq!(pending, 7);
}

#[test]
fn rpc_eth_state_reads_at_respect_blocktag_window() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let addr = [0x11u8; 20];
    let slot = [0x22u8; 32];
    let code = vec![0x60, 0x00, 0x56];
    let code_hash = hash::keccak256(&code);
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr),
            AccountVal::from_parts(0, [0x44u8; 32], code_hash),
        );
        state
            .codes
            .insert(make_code_key(code_hash), CodeVal(code.clone()));
        state
            .storage
            .insert(make_storage_key(addr, slot), U256Val([0x55u8; 32]));
        state.head.set(Head {
            number: 2,
            block_hash: [0x22u8; 32],
            timestamp: 1_700_000_002,
        });
        let mut prune = *state.prune_state.get();
        prune.set_pruned_before(0);
        state.prune_state.set(prune);
    });

    let bal_latest =
        rpc_eth_get_balance(addr.to_vec(), RpcBlockTagView::Latest).expect("latest balance");
    assert_eq!(bal_latest, [0x44u8; 32].to_vec());
    let code_head =
        rpc_eth_get_code(addr.to_vec(), RpcBlockTagView::Number(2)).expect("head-number code");
    assert_eq!(code_head, code);
    let storage_head =
        rpc_eth_get_storage_at(addr.to_vec(), slot.to_vec(), RpcBlockTagView::Number(2))
            .expect("head-number storage");
    assert_eq!(storage_head, [0x55u8; 32].to_vec());

    let bal_past = rpc_eth_get_balance(addr.to_vec(), RpcBlockTagView::Number(1))
        .expect_err("historical balance should be unavailable");
    assert_eq!(bal_past.code, 2001);
    assert!(bal_past.message.starts_with("exec.state.unavailable"));
    let code_past = rpc_eth_get_code(addr.to_vec(), RpcBlockTagView::Number(1))
        .expect_err("historical code should be unavailable");
    assert_eq!(code_past.code, 2001);
    assert!(code_past.message.starts_with("exec.state.unavailable"));
    let storage_past =
        rpc_eth_get_storage_at(addr.to_vec(), slot.to_vec(), RpcBlockTagView::Number(1))
            .expect_err("historical storage should be unavailable");
    assert_eq!(storage_past.code, 2001);
    assert!(storage_past.message.starts_with("exec.state.unavailable"));

    let bal_oow = rpc_eth_get_balance(addr.to_vec(), RpcBlockTagView::Number(3))
        .expect_err("out-of-window number should fail");
    assert_eq!(bal_oow.code, 1001);
    assert!(bal_oow
        .message
        .starts_with("invalid.block_range.out_of_window"));

    let earliest = rpc_eth_get_code(addr.to_vec(), RpcBlockTagView::Earliest)
        .expect_err("earliest should be out-of-window when pruned");
    assert_eq!(earliest.code, 1001);
    assert!(earliest
        .message
        .starts_with("invalid.block_range.out_of_window"));

    with_state_mut(|state| {
        let mut prune = *state.prune_state.get();
        prune.set_pruned_before(10);
        state.prune_state.set(prune);
    });
    let earliest_oow =
        rpc_eth_get_storage_at(addr.to_vec(), slot.to_vec(), RpcBlockTagView::Earliest)
            .expect_err("earliest should be out-of-window when pruned");
    assert_eq!(earliest_oow.code, 1001);
    assert!(earliest_oow
        .message
        .starts_with("invalid.block_range.out_of_window"));
}

#[test]
fn rpc_eth_call_and_estimate_at_reject_out_of_window_block() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    with_state_mut(|state| {
        let mut prune = *state.prune_state.get();
        prune.set_pruned_before(10);
        state.prune_state.set(prune);
    });
    let call = RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: None,
        gas: Some(30_000),
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: Some(vec![0u8; 32]),
        data: Some(Vec::new()),
    };
    let call_err = rpc_eth_call_object_at(call.clone(), RpcBlockTagView::Number(1))
        .expect_err("out of window call should fail");
    assert_eq!(call_err.code, 1001);
    assert!(call_err
        .message
        .starts_with("invalid.block_range.out_of_window"));

    let est_err = rpc_eth_estimate_gas_object_at(call, RpcBlockTagView::Earliest)
        .expect_err("out of window estimate should fail");
    assert_eq!(est_err.code, 1001);
    assert!(est_err
        .message
        .starts_with("invalid.block_range.out_of_window"));
}

#[test]
fn rpc_eth_call_and_estimate_at_accept_head_number_tag() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let from = [0x77u8; 20];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(from),
            AccountVal::from_parts(0, [0xffu8; 32], [0u8; 32]),
        );
        state.head.set(Head {
            number: 5,
            block_hash: [0x55u8; 32],
            timestamp: 1_700_000_005,
        });
    });
    let call = RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: Some(from.to_vec()),
        gas: Some(30_000),
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: Some(vec![0u8; 32]),
        data: Some(Vec::new()),
    };
    let call_out = rpc_eth_call_object_at(call.clone(), RpcBlockTagView::Number(5))
        .expect("head number call should succeed");
    assert_eq!(call_out.status, 1);
    let gas = rpc_eth_estimate_gas_object_at(call, RpcBlockTagView::Number(5))
        .expect("head number estimate should succeed");
    assert!(gas > 0);
}

#[test]
fn rpc_eth_call_object_at_async_resolves_icp_query_for_latest_tags() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let from = [0x78u8; 20];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(from),
            AccountVal::from_parts(0, [0xffu8; 32], [0u8; 32]),
        );
        state.head.set(Head {
            number: 5,
            block_hash: [0x55u8; 32],
            timestamp: 1_700_000_005,
        });
    });
    let call = icp_query_call_object(from, "read_state", &[0x44, 0x49, 0x44, 0x4c]);
    let expected_reply = vec![0xaa, 0xbb, 0xcc];
    let mut latest_called = false;

    let latest = run_ready_future(rpc_eth_call_object_at_async(
        call.clone(),
        RpcBlockTagView::Latest,
        |request| {
            latest_called = true;
            assert_eq!(request.method, "read_state");
            let reply = expected_reply.clone();
            async move { Ok(reply) }
        },
    ))
    .expect("latest async call");
    assert!(latest_called);
    assert_eq!(latest.status, 1);
    assert_eq!(latest.return_data, expected_reply);

    let mut number_called = false;
    let by_number = run_ready_future(rpc_eth_call_object_at_async(
        call,
        RpcBlockTagView::Number(5),
        |_| {
            number_called = true;
            async { Ok(vec![0xdd]) }
        },
    ))
    .expect("head-number async call");
    assert!(number_called);
    assert_eq!(by_number.status, 1);
    assert_eq!(by_number.return_data, vec![0xdd]);
}

#[test]
fn rpc_eth_call_object_at_async_keeps_resolver_error_as_revert_shape() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let from = [0x79u8; 20];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(from),
            AccountVal::from_parts(0, [0xffu8; 32], [0u8; 32]),
        );
    });
    let call = icp_query_call_object(from, "read_state", &[]);
    let mut resolver_called = false;

    let out = run_ready_future(rpc_eth_call_object_at_async(
        call,
        RpcBlockTagView::Latest,
        |_| {
            resolver_called = true;
            async { Err("ic_query.test_error".to_string()) }
        },
    ))
    .expect("resolver error should execute as precompile revert");

    assert!(resolver_called);
    assert_eq!(out.status, 0);
    assert!(out.return_data.is_empty());
    assert!(out.revert_data.is_none());
}

#[test]
fn rpc_eth_fee_methods_validate_and_window_is_exposed() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    with_state_mut(|state| {
        let mut prune = *state.prune_state.get();
        prune.set_pruned_before(5);
        state.prune_state.set(prune);
    });
    let window = rpc_eth_history_window();
    assert_eq!(window.oldest_available, 6);

    let fee_count_err = rpc_eth_fee_history(0, RpcBlockTagView::Latest, None)
        .expect_err("invalid block count should fail");
    assert_eq!(fee_count_err.code, 1001);
    assert!(fee_count_err
        .message
        .starts_with("invalid.fee_history.block_count"));

    let fee_pct_err = rpc_eth_fee_history(1, RpcBlockTagView::Latest, Some(vec![90.0, 10.0]))
        .expect_err("invalid percentiles should fail");
    assert_eq!(fee_pct_err.code, 1001);
    assert!(fee_pct_err
        .message
        .starts_with("invalid.fee_history.percentiles"));

    let tip_err = rpc_eth_max_priority_fee_per_gas()
        .expect_err("empty chain should return state unavailable");
    assert_eq!(tip_err.code, 2001);
    assert!(tip_err.message.starts_with("exec.state.unavailable"));

    let gas_price_err =
        rpc_eth_gas_price().expect_err("empty chain should return state unavailable");
    assert_eq!(gas_price_err.code, 2001);
    assert!(gas_price_err.message.starts_with("exec.state.unavailable"));
}

#[test]
fn rpc_eth_fee_history_is_deterministic_for_same_head() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let block = BlockData::new(
        1,
        [0u8; 32],
        [1u8; 32],
        1_700_000_000,
        1_000_000_000,
        3_000_000,
        0,
        [0x44; 20],
        Vec::new(),
        [2u8; 32],
        [3u8; 32],
    );
    with_state_mut(|state| {
        let ptr = state
            .blob_store
            .store_bytes(&block.clone().into_bytes())
            .expect("store block");
        state.blocks.insert(1, ptr);
        state.head.set(Head {
            number: 1,
            block_hash: block.block_hash,
            timestamp: block.timestamp,
        });
    });
    let a = rpc_eth_fee_history(1, RpcBlockTagView::Latest, Some(vec![50.0]))
        .expect("first fee history call");
    let b = rpc_eth_fee_history(1, RpcBlockTagView::Latest, Some(vec![50.0]))
        .expect("second fee history call");
    assert_eq!(a.oldest_block, b.oldest_block);
    assert_eq!(a.base_fee_per_gas, b.base_fee_per_gas);
    assert_eq!(a.gas_used_ratio, b.gas_used_ratio);
    assert_eq!(a.reward, b.reward);
}

#[test]
fn rpc_eth_gas_price_respects_min_gas_price_floor() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let block = BlockData::new(
        1,
        [0u8; 32],
        [1u8; 32],
        1_700_000_000,
        1_000_000_000,
        3_000_000,
        0,
        [0x44; 20],
        Vec::new(),
        [2u8; 32],
        [3u8; 32],
    );
    with_state_mut(|state| {
        let ptr = state
            .blob_store
            .store_bytes(&block.clone().into_bytes())
            .expect("store block");
        state.blocks.insert(1, ptr);
        state.head.set(Head {
            number: 1,
            block_hash: block.block_hash,
            timestamp: block.timestamp,
        });
        let mut chain_state = *state.chain_state.get();
        chain_state.min_priority_fee = 2_000_000_000;
        chain_state.min_gas_price = 10_000_000_000;
        state.chain_state.set(chain_state);
    });
    let gas_price = rpc_eth_gas_price().expect("gas price should be available");
    assert_eq!(gas_price, 10_000_000_000);
}

#[test]
fn rpc_eth_max_priority_fee_per_gas_respects_min_priority_fee_floor() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    store_fee_sample_block(2_000_000_000, 1_000_000_000);
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.min_priority_fee = 2_000_000_000;
        state.chain_state.set(chain_state);
    });
    let tip = rpc_eth_max_priority_fee_per_gas().expect("priority fee should be available");
    assert_eq!(tip, 2_000_000_000);
}

#[test]
fn rpc_eth_max_priority_fee_per_gas_uses_observed_value_when_above_floor() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 0;
        chain_state.min_gas_price = 0;
        state.chain_state.set(chain_state);
    });
    store_eth_signed_fee_sample_block(0, 4_000_000_000, 3_000_000_000);
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.min_priority_fee = 2_000_000_000;
        state.chain_state.set(chain_state);
    });
    let tip = rpc_eth_max_priority_fee_per_gas().expect("priority fee should be available");
    assert_eq!(tip, 3_000_000_000);
}

#[test]
fn rpc_eth_gas_price_respects_base_plus_min_priority_floor() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let block = BlockData::new(
        1,
        [0u8; 32],
        [1u8; 32],
        1_700_000_000,
        3_000_000_000,
        3_000_000,
        0,
        [0x44; 20],
        Vec::new(),
        [2u8; 32],
        [3u8; 32],
    );
    with_state_mut(|state| {
        let ptr = state
            .blob_store
            .store_bytes(&block.clone().into_bytes())
            .expect("store block");
        state.blocks.insert(1, ptr);
        state.head.set(Head {
            number: 1,
            block_hash: block.block_hash,
            timestamp: block.timestamp,
        });
        let mut chain_state = *state.chain_state.get();
        chain_state.min_priority_fee = 2_000_000_000;
        chain_state.min_gas_price = 1_000_000_000;
        state.chain_state.set(chain_state);
    });
    let gas_price = rpc_eth_gas_price().expect("gas price should be available");
    assert_eq!(gas_price, 5_000_000_000);
}

#[test]
fn rpc_fee_suggestions_ignore_ic_synthetic_only_head_block() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 0;
        chain_state.min_gas_price = 0;
        state.chain_state.set(chain_state);
    });
    store_eth_signed_fee_sample_block(0, 4_000_000_000, 3_000_000_000);
    store_fee_sample_block(120_000_000_000, 120_000_000_000);

    let tip = rpc_eth_max_priority_fee_per_gas().expect("priority fee should be available");
    let gas_price = rpc_eth_gas_price().expect("gas price should be available");
    assert_eq!(tip, 3_000_000_000);
    assert_eq!(gas_price, 4_000_000_000);
}

#[test]
fn rpc_fee_suggestions_fall_back_to_floor_when_no_eth_signed_samples_exist() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    store_fee_sample_block(120_000_000_000, 120_000_000_000);
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.min_priority_fee = 2_000_000_000;
        chain_state.min_gas_price = 10_000_000_000;
        state.chain_state.set(chain_state);
    });

    let tip = rpc_eth_max_priority_fee_per_gas().expect("priority fee should be available");
    let gas_price = rpc_eth_gas_price().expect("gas price should be available");
    assert_eq!(tip, 2_000_000_000);
    assert_eq!(gas_price, 10_000_000_000);
}

#[test]
fn rpc_fee_suggestions_ignore_ic_synthetic_when_window_contains_mixed_kinds() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 0;
        chain_state.min_gas_price = 0;
        state.chain_state.set(chain_state);
    });
    store_fee_sample_block(90_000_000_000, 90_000_000_000);
    store_eth_signed_fee_sample_block(0, 6_000_000_000, 5_000_000_000);

    let tip = rpc_eth_max_priority_fee_per_gas().expect("priority fee should be available");
    let gas_price = rpc_eth_gas_price().expect("gas price should be available");
    let head_base_fee = chain::get_block(chain::get_head_number())
        .expect("head block")
        .base_fee_per_gas as u128;
    assert_eq!(tip, 5_000_000_000);
    assert_eq!(gas_price, head_base_fee.saturating_add(tip));
}

#[test]
fn rpc_eth_fee_history_keeps_ic_synthetic_rewards_raw() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 0;
        chain_state.min_gas_price = 0;
        state.chain_state.set(chain_state);
    });
    store_fee_sample_block(90_000_000_000, 90_000_000_000);

    let history = rpc_eth_fee_history(1, RpcBlockTagView::Latest, Some(vec![50.0]))
        .expect("fee history should succeed");
    assert_eq!(history.reward.as_ref().map(Vec::len), Some(1));
    let reward = &history.reward.expect("reward exists")[0];
    assert_eq!(reward, &vec![89_000_000_000]);
}

#[test]
fn rpc_eth_call_object_rejects_bad_lengths() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let err = rpc_eth_call_object(RpcCallObjectView {
        to: Some(vec![0u8; 19]),
        from: None,
        gas: None,
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: None,
        data: None,
    })
    .expect_err("invalid to should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "to must be 20 bytes");

    let err = rpc_eth_call_object(RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: Some(vec![0u8; 19]),
        gas: None,
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: None,
        data: None,
    })
    .expect_err("invalid from should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "from must be 20 bytes");

    let err = rpc_eth_call_object(RpcCallObjectView {
        to: Some(vec![0u8; 32]),
        from: None,
        gas: None,
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: None,
        data: None,
    })
    .expect_err("bytes32-like to should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(
        err.message,
        "to must be 20 bytes (got 32; this looks like bytes32-encoded principal)"
    );

    let err = rpc_eth_call_object(RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: None,
        gas: None,
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: Some(vec![0u8; 31]),
        data: None,
    })
    .expect_err("invalid value should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "value must be 32 bytes");
}

#[test]
fn rpc_eth_call_object_rejects_fee_combination_errors() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let err = rpc_eth_call_object(RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: None,
        gas: None,
        gas_price: Some(1),
        nonce: None,
        max_fee_per_gas: Some(2),
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: None,
        data: None,
    })
    .expect_err("invalid fee combo should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(
        err.message,
        "gasPrice and maxFeePerGas/maxPriorityFeePerGas cannot be used together"
    );

    let err = rpc_eth_call_object(RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: None,
        gas: None,
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: Some(1),
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: None,
        data: None,
    })
    .expect_err("priority without max fee should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "maxPriorityFeePerGas requires maxFeePerGas");

    let err = rpc_eth_call_object(RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: None,
        gas: None,
        gas_price: None,
        nonce: None,
        max_fee_per_gas: Some(1),
        max_priority_fee_per_gas: Some(2),
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: None,
        data: None,
    })
    .expect_err("priority > max fee should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "maxPriorityFeePerGas must be <= maxFeePerGas");
}

#[test]
fn rpc_eth_call_object_rejects_type_and_chain_id_mismatch() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let err = rpc_eth_call_object(RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: None,
        gas: None,
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: Some(999),
        tx_type: None,
        access_list: None,
        value: None,
        data: None,
    })
    .expect_err("chain mismatch should fail");
    assert_eq!(err.code, 1001);
    assert!(err.message.starts_with("chainId mismatch: expected "));

    let err = rpc_eth_call_object(RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: None,
        gas: None,
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: Some(1),
        access_list: None,
        value: None,
        data: None,
    })
    .expect_err("unsupported type should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "type must be 0x0 or 0x2");
}

#[test]
fn rpc_eth_call_object_supports_nonce_type2_and_access_list() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let from = [0x44u8; 20];
    evm_db::stable_state::with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(from),
            AccountVal::from_parts(0, [0xffu8; 32], [0u8; 32]),
        );
    });
    let call = RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: Some(from.to_vec()),
        gas: Some(30_000),
        gas_price: None,
        nonce: Some(0),
        max_fee_per_gas: Some(u128::from(DEFAULT_BASE_FEE).saturating_add(1_000_000_000)),
        max_priority_fee_per_gas: Some(1_000_000_000),
        chain_id: Some(evm_db::chain_data::constants::CHAIN_ID),
        tx_type: Some(2),
        access_list: Some(vec![ic_evm_rpc_types::RpcAccessListItemView {
            address: vec![0u8; 20],
            storage_keys: vec![vec![0u8; 32]],
        }]),
        value: Some(vec![0u8; 32]),
        data: Some(Vec::new()),
    };
    let out = rpc_eth_call_object(call.clone()).expect("call with type2 should succeed");
    assert_eq!(out.status, 1);
    let gas = rpc_eth_estimate_gas_object(call).expect("estimate with type2 should succeed");
    assert!(gas > 0);
}

#[test]
fn rpc_eth_call_object_uses_account_nonce_when_nonce_omitted() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let from = [0x55u8; 20];
    evm_db::stable_state::with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(from),
            AccountVal::from_parts(9, [0xffu8; 32], [0u8; 32]),
        );
    });
    let call = RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: Some(from.to_vec()),
        gas: Some(30_000),
        gas_price: Some(
            u128::from(DEFAULT_BASE_FEE.max(DEFAULT_MIN_FEE_FLOOR)).saturating_add(1_000_000_000),
        ),
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: Some(vec![0u8; 32]),
        data: Some(Vec::new()),
    };
    let out = rpc_eth_call_object(call.clone()).expect("call should infer account nonce");
    assert_eq!(out.status, 1);
    let gas = rpc_eth_estimate_gas_object(call).expect("estimate should infer account nonce");
    assert!(gas > 0);
}

#[test]
fn rpc_eth_call_object_rejects_bad_access_list_lengths() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let err = rpc_eth_call_object(RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: None,
        gas: None,
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: Some(vec![ic_evm_rpc_types::RpcAccessListItemView {
            address: vec![0u8; 19],
            storage_keys: vec![],
        }]),
        value: None,
        data: None,
    })
    .expect_err("bad access list address should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "accessList.address must be 20 bytes");

    let err = rpc_eth_call_object(RpcCallObjectView {
        to: Some(vec![0u8; 20]),
        from: None,
        gas: None,
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: Some(vec![ic_evm_rpc_types::RpcAccessListItemView {
            address: vec![0u8; 20],
            storage_keys: vec![vec![0u8; 31]],
        }]),
        value: None,
        data: None,
    })
    .expect_err("bad access list slot should fail");
    assert_eq!(err.code, 1001);
    assert_eq!(err.message, "accessList.storageKeys[] must be 32 bytes");
}

#[test]
fn submit_tx_maps_decode_error_to_invalid_argument() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let err = rpc_eth_send_raw_transaction(Vec::new(), Vec::new())
        .expect_err("invalid tx bytes should fail");
    match err {
        ic_evm_rpc_types::SubmitTxError::InvalidArgument(code) => {
            assert_eq!(code, "arg.decode_failed");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn submit_tx_maps_too_large_error_to_invalid_argument() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let oversized = vec![0u8; MAX_TX_SIZE + 1];
    let err =
        rpc_eth_send_raw_transaction(oversized, Vec::new()).expect_err("oversized tx should fail");
    match err {
        ic_evm_rpc_types::SubmitTxError::InvalidArgument(code) => {
            assert_eq!(code, "arg.tx_too_large");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn call_rawtx_keeps_error_surface_stable() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let err = rpc_eth_call_rawtx(Vec::new()).expect_err("invalid call tx should fail");
    assert!(err.starts_with("eth_call failed:"));
}

#[test]
fn submit_tx_in_with_code_keeps_decode_mapping() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let err = submit_tx_in_with_code(
        evm_core::chain::TxIn::EthSigned {
            tx_bytes: Vec::new(),
            caller_principal: Vec::new(),
        },
        "rpc_eth_send_raw_transaction",
    )
    .expect_err("invalid tx bytes should fail");
    match err {
        ic_evm_rpc_types::SubmitTxError::InvalidArgument(code) => {
            assert_eq!(code, "arg.decode_failed");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn get_block_by_number_hashes_prefers_eth_tx_hash_for_eth_signed() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    let raw = vec![0x02, 0x99, 0xaa, 0xbb];
    let tx_id = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw,
        None,
        None,
        None,
    ));
    let stored = StoredTxBytes::new_with_fees(
        tx_id,
        TxKind::EthSigned,
        raw.clone(),
        None,
        Vec::new(),
        Vec::new(),
        0,
        0,
        false,
    );
    let block = BlockData::new(
        1,
        [0u8; 32],
        [1u8; 32],
        1_700_000_000,
        1_000_000_000,
        3_000_000,
        21_000,
        [0x44; 20],
        vec![tx_id],
        [2u8; 32],
        [3u8; 32],
    );
    with_state_mut(|state| {
        state.tx_store.insert(tx_id, stored);
        let ptr = state
            .blob_store
            .store_bytes(&block.clone().into_bytes())
            .expect("store block");
        state.blocks.insert(1, ptr);
    });

    let out = rpc_eth_get_block_by_number_with_status(1, false);
    match out {
        RpcBlockLookupView::Found(block_view) => match block_view.txs {
            ic_evm_rpc_types::EthTxListView::Hashes(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0], hash::keccak256(&raw).to_vec());
                assert_eq!(block_view.beneficiary, vec![0x44; 20]);
            }
            other => panic!("unexpected tx list shape: {other:?}"),
        },
        other => panic!("unexpected block lookup status: {other:?}"),
    }
}

#[test]
fn get_block_number_by_hash_finds_block_within_scan_window() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    let block = BlockData::new(
        3,
        [0u8; 32],
        [0x11u8; 32],
        1_700_000_000,
        1_000_000_000,
        3_000_000,
        21_000,
        [0x44; 20],
        vec![],
        [2u8; 32],
        [3u8; 32],
    );
    with_state_mut(|state| {
        let ptr = state
            .blob_store
            .store_bytes(&block.clone().into_bytes())
            .expect("store block");
        state.blocks.insert(3, ptr);
        state.head.set(evm_db::chain_data::Head {
            number: 3,
            block_hash: [0x11u8; 32],
            timestamp: 1_700_000_000,
        });
    });

    let found = rpc_eth_get_block_number_by_hash([0x11u8; 32].to_vec(), 10).expect("lookup");
    assert_eq!(found, Some(3));
}

#[test]
fn get_block_number_by_hash_respects_scan_window() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    for number in 1..=3u64 {
        let block_hash = [number as u8; 32];
        let block = BlockData::new(
            number,
            [0u8; 32],
            block_hash,
            1_700_000_000 + number,
            1_000_000_000,
            3_000_000,
            21_000,
            [0x44; 20],
            vec![],
            [2u8; 32],
            [3u8; 32],
        );
        with_state_mut(|state| {
            let ptr = state
                .blob_store
                .store_bytes(&block.clone().into_bytes())
                .expect("store block");
            state.blocks.insert(number, ptr);
        });
    }
    with_state_mut(|state| {
        state.head.set(evm_db::chain_data::Head {
            number: 3,
            block_hash: [3u8; 32],
            timestamp: 1_700_000_003,
        });
    });

    let not_found = rpc_eth_get_block_number_by_hash([1u8; 32].to_vec(), 2).expect("lookup");
    assert_eq!(not_found, None);
    let found = rpc_eth_get_block_number_by_hash([1u8; 32].to_vec(), 3).expect("lookup");
    assert_eq!(found, Some(1));
}

#[test]
fn get_block_number_by_hash_rejects_invalid_hash_length() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let err = rpc_eth_get_block_number_by_hash(vec![0u8; 31], 10).expect_err("invalid hash");
    assert_eq!(err, "block_hash must be 32 bytes");
}

#[test]
fn get_transaction_by_hash_reads_from_eth_hash_index() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    let raw = vec![0x02, 0xaa, 0xbb, 0xcc];
    let tx_id = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw,
        None,
        None,
        None,
    ));
    let eth_hash = hash::keccak256(&raw);
    let stored = StoredTxBytes::new_with_fees(
        tx_id,
        TxKind::EthSigned,
        raw,
        None,
        Vec::new(),
        Vec::new(),
        0,
        0,
        false,
    );
    with_state_mut(|state| {
        state.tx_store.insert(tx_id, stored);
        state.eth_tx_hash_index.insert(TxId(eth_hash), tx_id);
    });

    let out = rpc_eth_get_transaction_by_eth_hash(eth_hash.to_vec()).expect("tx must exist");
    assert_eq!(out.hash, tx_id.0.to_vec());
}

#[test]
fn get_transaction_by_hash_returns_none_on_index_miss() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    let raw = vec![0x02, 0xdd, 0xee, 0xff];
    let tx_id = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw,
        None,
        None,
        None,
    ));
    let stored = StoredTxBytes::new_with_fees(
        tx_id,
        TxKind::EthSigned,
        raw.clone(),
        None,
        Vec::new(),
        Vec::new(),
        0,
        0,
        false,
    );
    with_state_mut(|state| {
        state.tx_store.insert(tx_id, stored);
    });

    let out = rpc_eth_get_transaction_by_eth_hash(hash::keccak256(&raw).to_vec());
    assert!(out.is_none());
}

#[test]
fn get_transaction_by_hash_returns_none_on_index_hash_mismatch() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    let raw_a = vec![0x02, 0xa1];
    let raw_b = vec![0x02, 0xb2];
    let tx_id_b = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw_b,
        None,
        None,
        None,
    ));
    let hash_a = hash::keccak256(&raw_a);
    let stored_b = StoredTxBytes::new_with_fees(
        tx_id_b,
        TxKind::EthSigned,
        raw_b,
        None,
        Vec::new(),
        Vec::new(),
        0,
        0,
        false,
    );
    with_state_mut(|state| {
        state.tx_store.insert(tx_id_b, stored_b);
        state.eth_tx_hash_index.insert(TxId(hash_a), tx_id_b);
    });

    let out = rpc_eth_get_transaction_by_eth_hash(hash_a.to_vec());
    assert!(out.is_none());
}

#[test]
fn get_transaction_receipt_has_block_wide_log_index() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    let raw0 = vec![0x02, 0x10];
    let raw1 = vec![0x02, 0x11];
    let tx0 = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw0,
        None,
        None,
        None,
    ));
    let tx1 = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw1,
        None,
        None,
        None,
    ));
    let stored0 = StoredTxBytes::new_with_fees(
        tx0,
        TxKind::EthSigned,
        raw0,
        None,
        Vec::new(),
        Vec::new(),
        0,
        0,
        false,
    );
    let stored1 = StoredTxBytes::new_with_fees(
        tx1,
        TxKind::EthSigned,
        raw1,
        None,
        Vec::new(),
        Vec::new(),
        0,
        0,
        false,
    );
    let block = BlockData::new(
        7,
        [0u8; 32],
        [7u8; 32],
        1_700_000_007,
        1_000_000_000,
        3_000_000,
        42_000,
        [0u8; 20],
        vec![tx0, tx1],
        [8u8; 32],
        [9u8; 32],
    );
    let receipt0 = ReceiptLike {
        tx_id: tx0,
        block_number: 7,
        tx_index: 0,
        status: 1,
        gas_used: 21_000,
        effective_gas_price: 1,
        l1_data_fee: 0,
        operator_fee: 0,
        total_fee: 0,
        return_data_hash: [0u8; 32],
        return_data: Vec::new(),
        contract_address: None,
        logs: vec![
            log_entry_from_parts([0x11; 20], vec![[0x22; 32]], vec![0xaa]),
            log_entry_from_parts([0x11; 20], vec![[0x23; 32]], vec![0xbb]),
        ],
    };
    let receipt1 = ReceiptLike {
        tx_id: tx1,
        block_number: 7,
        tx_index: 1,
        status: 1,
        gas_used: 21_000,
        effective_gas_price: 1,
        l1_data_fee: 0,
        operator_fee: 0,
        total_fee: 0,
        return_data_hash: [0u8; 32],
        return_data: Vec::new(),
        contract_address: None,
        logs: vec![log_entry_from_parts(
            [0x12; 20],
            vec![[0x24; 32]],
            vec![0xcc],
        )],
    };
    with_state_mut(|state| {
        state.tx_store.insert(tx0, stored0);
        state.tx_store.insert(tx1, stored1);
        state
            .eth_tx_hash_index
            .insert(TxId(hash::keccak256(&[0x02, 0x10])), tx0);
        state
            .eth_tx_hash_index
            .insert(TxId(hash::keccak256(&[0x02, 0x11])), tx1);

        let block_ptr = state
            .blob_store
            .store_bytes(&block.clone().into_bytes())
            .expect("store block");
        state.blocks.insert(7, block_ptr);

        let receipt0_ptr = state
            .blob_store
            .store_bytes(&receipt0.clone().into_bytes())
            .expect("store receipt0");
        let receipt1_ptr = state
            .blob_store
            .store_bytes(&receipt1.clone().into_bytes())
            .expect("store receipt1");
        state.receipts.insert(tx0, receipt0_ptr);
        state.receipts.insert(tx1, receipt1_ptr);

        let tx_index0 = TxIndexEntry {
            block_number: 7,
            tx_index: 0,
        };
        let tx_index1 = TxIndexEntry {
            block_number: 7,
            tx_index: 1,
        };
        let tx_index0_ptr = state
            .blob_store
            .store_bytes(&tx_index0.into_bytes())
            .expect("store tx index0");
        let tx_index1_ptr = state
            .blob_store
            .store_bytes(&tx_index1.into_bytes())
            .expect("store tx index1");
        state.tx_index.insert(tx0, tx_index0_ptr);
        state.tx_index.insert(tx1, tx_index1_ptr);
    });

    let out = rpc_eth_get_transaction_receipt_by_eth_hash(hash::keccak256(&[0x02, 0x11]).to_vec())
        .expect("receipt must exist");
    assert_eq!(out.logs.len(), 1);
    assert_eq!(out.logs[0].log_index, 2);
    assert_eq!(out.cumulative_gas_used, Some(42_000));
}

#[test]
fn get_logs_paged_uses_block_wide_indexes_and_next_cursor() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    let raw0 = vec![0x02, 0x20];
    let raw1 = vec![0x02, 0x21];
    let tx0 = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw0,
        None,
        None,
        None,
    ));
    let tx1 = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw1,
        None,
        None,
        None,
    ));
    let block = BlockData::new(
        8,
        [0u8; 32],
        [0x88u8; 32],
        1_700_000_008,
        1_000_000_000,
        3_000_000,
        42_000,
        [0u8; 20],
        vec![tx0, tx1],
        [8u8; 32],
        [9u8; 32],
    );
    let receipt0 = ReceiptLike {
        tx_id: tx0,
        block_number: 8,
        tx_index: 0,
        status: 1,
        gas_used: 21_000,
        effective_gas_price: 1,
        l1_data_fee: 0,
        operator_fee: 0,
        total_fee: 0,
        return_data_hash: [0u8; 32],
        return_data: Vec::new(),
        contract_address: None,
        logs: vec![
            log_entry_from_parts([0x11; 20], vec![[0x22; 32]], vec![0xaa]),
            log_entry_from_parts([0x11; 20], vec![[0x23; 32]], vec![0xbb]),
        ],
    };
    let receipt1 = ReceiptLike {
        tx_id: tx1,
        block_number: 8,
        tx_index: 1,
        status: 1,
        gas_used: 21_000,
        effective_gas_price: 1,
        l1_data_fee: 0,
        operator_fee: 0,
        total_fee: 0,
        return_data_hash: [0u8; 32],
        return_data: Vec::new(),
        contract_address: None,
        logs: vec![log_entry_from_parts(
            [0x12; 20],
            vec![[0x24; 32]],
            vec![0xcc],
        )],
    };
    with_state_mut(|state| {
        state.tx_store.insert(
            tx0,
            StoredTxBytes::new_with_fees(
                tx0,
                TxKind::EthSigned,
                raw0,
                None,
                Vec::new(),
                Vec::new(),
                0,
                0,
                false,
            ),
        );
        state.tx_store.insert(
            tx1,
            StoredTxBytes::new_with_fees(
                tx1,
                TxKind::EthSigned,
                raw1,
                None,
                Vec::new(),
                Vec::new(),
                0,
                0,
                false,
            ),
        );
        let block_ptr = state
            .blob_store
            .store_bytes(&block.clone().into_bytes())
            .expect("store block");
        state.blocks.insert(8, block_ptr);
        state.head.set(Head {
            number: 8,
            block_hash: [0x88; 32],
            timestamp: 1_700_000_008,
        });
        let receipt0_ptr = state
            .blob_store
            .store_bytes(&receipt0.clone().into_bytes())
            .expect("store receipt0");
        let receipt1_ptr = state
            .blob_store
            .store_bytes(&receipt1.clone().into_bytes())
            .expect("store receipt1");
        state.receipts.insert(tx0, receipt0_ptr);
        state.receipts.insert(tx1, receipt1_ptr);
    });

    let filter = EthLogFilterView {
        from_block: Some(8),
        to_block: Some(8),
        address: None,
        topic0: None,
        topic1: None,
        limit: None,
    };
    let page0 = rpc_eth_get_logs_paged(filter.clone(), None, 1).expect("page0");
    assert_eq!(page0.items.len(), 1);
    assert_eq!(page0.items[0].log_index, 0);
    assert_eq!(page0.items[0].block_hash, Some(vec![0x88; 32]));

    let page1 = rpc_eth_get_logs_paged(filter.clone(), page0.next_cursor, 1).expect("page1");
    assert_eq!(page1.items.len(), 1);
    assert_eq!(page1.items[0].log_index, 1);

    let page2 = rpc_eth_get_logs_paged(filter, page1.next_cursor, 1).expect("page2");
    assert_eq!(page2.items.len(), 1);
    assert_eq!(page2.items[0].log_index, 2);
}

#[test]
fn get_transaction_receipt_with_status_by_eth_hash_accepts_eth_hash() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    let raw = vec![0x02, 0x44];
    let tx_id = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw,
        None,
        None,
        None,
    ));
    let eth_hash = hash::keccak256(&raw);
    let stored = StoredTxBytes::new_with_fees(
        tx_id,
        TxKind::EthSigned,
        raw,
        None,
        Vec::new(),
        Vec::new(),
        0,
        0,
        false,
    );
    let receipt = ReceiptLike {
        tx_id,
        block_number: 9,
        tx_index: 0,
        status: 1,
        gas_used: 21_000,
        effective_gas_price: 1,
        l1_data_fee: 0,
        operator_fee: 0,
        total_fee: 0,
        return_data_hash: [0u8; 32],
        return_data: Vec::new(),
        contract_address: None,
        logs: vec![],
    };
    with_state_mut(|state| {
        state.tx_store.insert(tx_id, stored);
        state.eth_tx_hash_index.insert(TxId(eth_hash), tx_id);
        let receipt_ptr = state
            .blob_store
            .store_bytes(&receipt.clone().into_bytes())
            .expect("store receipt");
        state.receipts.insert(tx_id, receipt_ptr);
    });

    let out = rpc_eth_get_transaction_receipt_with_status_by_eth_hash(eth_hash.to_vec());
    match out {
        RpcReceiptLookupView::Found(found) => {
            assert_eq!(found.as_ref().tx_hash, tx_id.0.to_vec());
            assert_eq!(found.as_ref().status, 1);
        }
        _ => panic!("expected Found for eth hash input"),
    }
}

#[test]
fn get_transaction_receipt_with_status_by_tx_id_accepts_tx_id() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    let raw = vec![0x02, 0x54];
    let tx_id = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw,
        None,
        None,
        None,
    ));
    let stored = StoredTxBytes::new_with_fees(
        tx_id,
        TxKind::EthSigned,
        raw,
        None,
        Vec::new(),
        Vec::new(),
        0,
        0,
        false,
    );
    let receipt = ReceiptLike {
        tx_id,
        block_number: 11,
        tx_index: 0,
        status: 1,
        gas_used: 21_000,
        effective_gas_price: 1,
        l1_data_fee: 0,
        operator_fee: 0,
        total_fee: 0,
        return_data_hash: [0u8; 32],
        return_data: Vec::new(),
        contract_address: None,
        logs: vec![],
    };
    with_state_mut(|state| {
        state.tx_store.insert(tx_id, stored);
        let receipt_ptr = state
            .blob_store
            .store_bytes(&receipt.clone().into_bytes())
            .expect("store receipt");
        state.receipts.insert(tx_id, receipt_ptr);
    });

    let out = rpc_eth_get_transaction_receipt_with_status_by_tx_id(tx_id.0.to_vec());
    match out {
        RpcReceiptLookupView::Found(found) => {
            assert_eq!(found.as_ref().tx_hash, tx_id.0.to_vec());
            assert_eq!(found.as_ref().status, 1);
        }
        _ => panic!("expected Found for tx_id input"),
    }
}

#[test]
fn get_transaction_receipt_with_status_by_tx_id_rejects_invalid_len() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    assert!(matches!(
        rpc_eth_get_transaction_receipt_with_status_by_tx_id(vec![0u8; 31]),
        RpcReceiptLookupView::NotFound
    ));
}

#[test]
fn get_transaction_receipt_by_hash_returns_none_on_index_hash_mismatch() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    let raw_a = vec![0x02, 0xc1];
    let raw_b = vec![0x02, 0xd2];
    let tx_id_b = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw_b,
        None,
        None,
        None,
    ));
    let hash_a = hash::keccak256(&raw_a);
    let stored_b = StoredTxBytes::new_with_fees(
        tx_id_b,
        TxKind::EthSigned,
        raw_b,
        None,
        Vec::new(),
        Vec::new(),
        0,
        0,
        false,
    );
    let receipt_b = ReceiptLike {
        tx_id: tx_id_b,
        block_number: 10,
        tx_index: 0,
        status: 1,
        gas_used: 21_000,
        effective_gas_price: 1,
        l1_data_fee: 0,
        operator_fee: 0,
        total_fee: 0,
        return_data_hash: [0u8; 32],
        return_data: Vec::new(),
        contract_address: None,
        logs: vec![],
    };
    with_state_mut(|state| {
        state.tx_store.insert(tx_id_b, stored_b);
        state.eth_tx_hash_index.insert(TxId(hash_a), tx_id_b);
        let receipt_ptr = state
            .blob_store
            .store_bytes(&receipt_b.into_bytes())
            .expect("store receipt");
        state.receipts.insert(tx_id_b, receipt_ptr);
    });

    assert!(rpc_eth_get_transaction_receipt_by_eth_hash(hash_a.to_vec()).is_none());
    assert!(matches!(
        rpc_eth_get_transaction_receipt_with_status_by_eth_hash(hash_a.to_vec()),
        RpcReceiptLookupView::NotFound
    ));
}
