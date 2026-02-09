//! どこで: ic-evm-rpc の統合テスト
//! 何を: 実運用で使うRPC経路のエラーマッピングと基本挙動を検証
//! なぜ: wrapper側テストと実運用実装の乖離を防ぐため

use evm_db::chain_data::constants::MAX_TX_SIZE;
use evm_db::stable_state::init_stable_state;
use ic_evm_rpc::{
    rpc_eth_call_rawtx, rpc_eth_get_balance, rpc_eth_get_code, rpc_eth_send_raw_transaction,
    submit_tx_in_with_code,
};
use std::sync::{Mutex, OnceLock};

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn rpc_eth_get_balance_rejects_invalid_address_length() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let err = rpc_eth_get_balance(vec![0u8; 19]).expect_err("invalid address should fail");
    assert_eq!(err, "address must be 20 bytes");
}

#[test]
fn rpc_eth_get_balance_returns_zero_for_unknown_account() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let out = rpc_eth_get_balance(vec![0u8; 20]).expect("query should succeed");
    assert_eq!(out, vec![0u8; 32]);
}

#[test]
fn rpc_eth_get_code_rejects_invalid_address_length() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let err = rpc_eth_get_code(vec![0u8; 21]).expect_err("invalid address should fail");
    assert_eq!(err, "address must be 20 bytes");
}

#[test]
fn rpc_eth_get_code_returns_empty_for_unknown_account() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let out = rpc_eth_get_code(vec![0u8; 20]).expect("query should succeed");
    assert!(out.is_empty());
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
    let err = rpc_eth_send_raw_transaction(oversized, Vec::new())
        .expect_err("oversized tx should fail");
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
        "submit_eth_tx",
    )
    .expect_err("invalid tx bytes should fail");
    match err {
        ic_evm_rpc_types::SubmitTxError::InvalidArgument(code) => {
            assert_eq!(code, "arg.decode_failed");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}
