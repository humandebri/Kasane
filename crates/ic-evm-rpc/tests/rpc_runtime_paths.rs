//! どこで: ic-evm-rpc の統合テスト
//! 何を: 実運用で使うRPC経路のエラーマッピングと基本挙動を検証
//! なぜ: wrapper側テストと実運用実装の乖離を防ぐため

use evm_core::hash;
use evm_db::chain_data::constants::MAX_TX_SIZE;
use evm_db::chain_data::receipt::log_entry_from_parts;
use evm_db::chain_data::runtime_defaults::{DEFAULT_BASE_FEE, DEFAULT_MIN_GAS_PRICE};
use evm_db::chain_data::{BlockData, ReceiptLike, StoredTxBytes, TxId, TxIndexEntry, TxKind};
use evm_db::stable_state::{init_stable_state, with_state_mut};
use evm_db::types::keys::{make_account_key, make_storage_key};
use evm_db::types::values::{AccountVal, U256Val};
use evm_db::Storable;
use ic_evm_rpc::{
    rpc_eth_call_object, rpc_eth_call_rawtx, rpc_eth_estimate_gas_object, rpc_eth_get_balance,
    rpc_eth_get_block_by_number_with_status, rpc_eth_get_code, rpc_eth_get_storage_at,
    rpc_eth_get_transaction_by_eth_hash, rpc_eth_get_transaction_receipt_by_eth_hash,
    rpc_eth_get_transaction_receipt_with_status, rpc_eth_send_raw_transaction,
    submit_tx_in_with_code,
};
use ic_evm_rpc_types::{RpcBlockLookupView, RpcCallObjectView, RpcReceiptLookupView};
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
    let err = rpc_eth_get_balance(vec![0u8; 32]).expect_err("bytes32-like address should fail");
    assert_eq!(
        err,
        "address must be 20 bytes (got 32; this looks like bytes32-encoded principal)"
    );
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
    let err = rpc_eth_get_storage_at(vec![0u8; 19], vec![0u8; 32])
        .expect_err("invalid address should fail");
    assert_eq!(err, "address must be 20 bytes");
    let err =
        rpc_eth_get_storage_at(vec![0u8; 20], vec![0u8; 31]).expect_err("invalid slot should fail");
    assert_eq!(err, "slot must be 32 bytes");
}

#[test]
fn rpc_eth_get_storage_at_returns_zero_and_reads_existing_value() {
    let _guard = test_lock().lock().expect("lock");
    init_stable_state();

    let missing =
        rpc_eth_get_storage_at(vec![0u8; 20], vec![0u8; 32]).expect("query should succeed");
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
        gas_price: Some(u128::from(DEFAULT_MIN_GAS_PRICE).saturating_add(1_000_000_000)),
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
}

#[test]
fn get_transaction_receipt_with_status_accepts_eth_hash() {
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

    let out = rpc_eth_get_transaction_receipt_with_status(eth_hash.to_vec());
    match out {
        RpcReceiptLookupView::Found(found) => {
            assert_eq!(found.tx_hash, tx_id.0.to_vec());
            assert_eq!(found.status, 1);
        }
        _ => panic!("expected Found for eth hash input"),
    }
}
