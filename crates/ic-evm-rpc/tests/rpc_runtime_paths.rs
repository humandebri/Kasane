//! どこで: ic-evm-rpc の統合テスト
//! 何を: 実運用で使うRPC経路のエラーマッピングと基本挙動を検証
//! なぜ: wrapper側テストと実運用実装の乖離を防ぐため

use evm_db::chain_data::constants::MAX_TX_SIZE;
use evm_db::stable_state::init_stable_state;
use evm_db::types::keys::{make_account_key, make_storage_key};
use evm_db::types::values::{AccountVal, U256Val};
use ic_evm_rpc::{
    rpc_eth_call_object, rpc_eth_call_rawtx, rpc_eth_estimate_gas_object, rpc_eth_get_balance,
    rpc_eth_get_code, rpc_eth_get_storage_at, rpc_eth_send_raw_transaction, submit_tx_in_with_code,
};
use ic_evm_rpc_types::RpcCallObjectView;
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
fn rpc_eth_get_storage_at_rejects_invalid_lengths() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();
    let err = rpc_eth_get_storage_at(vec![0u8; 19], vec![0u8; 32]).expect_err("invalid address should fail");
    assert_eq!(err, "address must be 20 bytes");
    let err = rpc_eth_get_storage_at(vec![0u8; 20], vec![0u8; 31]).expect_err("invalid slot should fail");
    assert_eq!(err, "slot must be 32 bytes");
}

#[test]
fn rpc_eth_get_storage_at_returns_zero_and_reads_existing_value() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    let missing = rpc_eth_get_storage_at(vec![0u8; 20], vec![0u8; 32]).expect("query should succeed");
    assert_eq!(missing, vec![0u8; 32]);

    let addr = [0x11u8; 20];
    let slot = [0x22u8; 32];
    evm_db::stable_state::with_state_mut(|state| {
        state
            .storage
            .insert(make_storage_key(addr, slot), U256Val([0x33u8; 32]));
    });
    let out = rpc_eth_get_storage_at(addr.to_vec(), slot.to_vec()).expect("query should succeed");
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
        max_fee_per_gas: Some(2_000_000_000),
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
        gas_price: Some(2_000_000_000),
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
