//! どこで: evm-core integration tests / 何を: テスト補助関数 / なぜ: 重複を減らし変更点を1箇所に集約するため

#![allow(dead_code)]

use evm_core::hash;
use evm_core::tx_decode::{encode_ic_synthetic_input, IcSyntheticTxInput};
use evm_db::chain_data::{
    CallerKey, ReadySeqKey, ReceiptLike, StoredTx, TxId, TxIndexEntry, TxLocKind,
};
use evm_db::stable_state::{with_state, with_state_mut};
use evm_db::types::keys::{make_account_key, make_code_key};
use evm_db::types::values::{AccountVal, CodeVal};
use evm_db::Storable;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

pub fn run_ready_future<F>(future: F) -> F::Output
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

pub fn build_ic_tx_bytes(
    to: [u8; 20],
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) -> Vec<u8> {
    encode_ic_synthetic_input(&build_ic_tx_input(
        to,
        nonce,
        max_fee_per_gas,
        max_priority_fee_per_gas,
    ))
}

pub fn build_ic_tx_input(
    to: [u8; 20],
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) -> IcSyntheticTxInput {
    IcSyntheticTxInput {
        to: Some(to),
        value: [0u8; 32],
        gas_limit: 50_000,
        nonce,
        max_fee_per_gas,
        max_priority_fee_per_gas,
        data: Vec::new(),
    }
}

pub fn build_default_ic_tx_bytes(nonce: u64) -> Vec<u8> {
    build_ic_tx_bytes([0x10u8; 20], nonce, 2_000_000_000, 1_000_000_000)
}

pub fn build_default_ic_tx_input(nonce: u64) -> IcSyntheticTxInput {
    build_ic_tx_input([0x10u8; 20], nonce, 2_000_000_000, 1_000_000_000)
}

pub fn build_zero_to_ic_tx_bytes(
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) -> Vec<u8> {
    build_ic_tx_bytes([0u8; 20], nonce, max_fee_per_gas, max_priority_fee_per_gas)
}

pub fn build_zero_to_ic_tx_input(
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) -> IcSyntheticTxInput {
    build_ic_tx_input([0u8; 20], nonce, max_fee_per_gas, max_priority_fee_per_gas)
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

pub fn fund_account(address: [u8; 20], amount: u128) {
    evm_core::chain::credit_balance(address, amount).expect("fund account");
}

pub fn execute_ic_tx_via_produce(
    caller_principal: Vec<u8>,
    canister_id: Vec<u8>,
    tx: IcSyntheticTxInput,
) -> (TxId, ReceiptLike) {
    let tx_id = evm_core::chain::submit_tx_in(evm_core::chain::TxIn::IcSynthetic {
        caller_principal,
        canister_id,
        tx,
    })
    .expect("submit");
    let outcome = evm_core::chain::produce_block(1).expect("produce");
    assert_eq!(outcome.block.tx_ids.len(), 1);
    assert_eq!(outcome.block.tx_ids[0], tx_id);
    let receipt = evm_core::chain::get_receipt(&tx_id).expect("receipt");
    (tx_id, receipt)
}

pub fn assert_block_persist_invariants(block_number: u64, tx_ids: &[TxId]) {
    with_state(|state| {
        let included_in_locs = state
            .tx_locs
            .iter()
            .filter(|entry| {
                let loc = entry.value();
                loc.kind == TxLocKind::Included && loc.block_number == block_number
            })
            .count();
        assert_eq!(included_in_locs, tx_ids.len());

        for (idx, tx_id) in tx_ids.iter().enumerate() {
            let expected_index = u32::try_from(idx).expect("test block index fits u32");
            let loc = state.tx_locs.get(tx_id).expect("included tx_loc");
            assert_eq!(loc.kind, TxLocKind::Included);
            assert_eq!(loc.block_number, block_number);
            assert_eq!(loc.tx_index, expected_index);

            let receipt_ptr = state.receipts.get(tx_id).expect("receipt ptr");
            let receipt_bytes = state.blob_store.read(&receipt_ptr).expect("receipt bytes");
            let receipt = ReceiptLike::from_bytes(Cow::Owned(receipt_bytes));
            assert_eq!(receipt.tx_id, *tx_id);
            assert_eq!(receipt.block_number, block_number);
            assert_eq!(receipt.tx_index, expected_index);

            let index_ptr = state.tx_index.get(tx_id).expect("tx_index ptr");
            let index_bytes = state.blob_store.read(&index_ptr).expect("tx_index bytes");
            let index = TxIndexEntry::from_bytes(Cow::Owned(index_bytes));
            assert_eq!(index.block_number, block_number);
            assert_eq!(index.tx_index, expected_index);

            assert_no_pending_or_ready_refs(*tx_id);
        }
    });
}

pub fn assert_dropped_tx_purged(tx_id: TxId, drop_code: u16) {
    with_state(|state| {
        let loc = state.tx_locs.get(&tx_id).expect("dropped tx_loc");
        assert_eq!(loc.kind, TxLocKind::Dropped);
        assert_eq!(loc.drop_code, drop_code);
        assert!(state.tx_store.get(&tx_id).is_none());
        assert!(state.pending_meta_by_tx_id.get(&tx_id).is_none());
        assert_no_pending_or_ready_refs(tx_id);
    });
}

pub fn assert_runtime_indexes_match_pending() {
    with_state(|state| {
        let mut expected_principal_counts: BTreeMap<CallerKey, u64> = BTreeMap::new();
        let mut pending_count = 0u64;

        for entry in state.pending_by_sender_nonce.iter() {
            let pending_key = *entry.key();
            let tx_id = entry.value();
            pending_count = pending_count.saturating_add(1);
            assert_eq!(state.pending_meta_by_tx_id.get(&tx_id), Some(pending_key));

            let stored = state.tx_store.get(&tx_id).expect("pending tx_store");
            let stored = StoredTx::try_from(stored).expect("pending tx decodes");
            if !stored.caller_principal.is_empty() {
                let key = CallerKey::from_principal_bytes(&stored.caller_principal);
                let current = expected_principal_counts.get(&key).copied().unwrap_or(0);
                expected_principal_counts.insert(key, current.saturating_add(1));
            }

            let fee_key = state
                .pending_fee_key_by_tx_id
                .get(&tx_id)
                .expect("pending fee key");
            assert_eq!(state.pending_fee_index.get(&fee_key), Some(tx_id));
        }

        assert_eq!(state.pending_fee_key_by_tx_id.len(), pending_count);
        assert_eq!(state.pending_fee_index.len(), pending_count);
        assert_eq!(state.pending_meta_by_tx_id.len(), pending_count);

        let actual_principal_counts: BTreeMap<CallerKey, u64> = state
            .principal_pending_count
            .iter()
            .map(|entry| (*entry.key(), u64::from(entry.value())))
            .collect();
        assert_eq!(actual_principal_counts, expected_principal_counts);

        for entry in state.ready_key_by_tx_id.iter() {
            let tx_id = *entry.key();
            let ready_key = entry.value();
            assert_eq!(state.ready_queue.get(&ready_key), Some(tx_id));
            assert_eq!(
                state
                    .ready_by_seq
                    .get(&ReadySeqKey::new(ready_key.seq(), tx_id.0)),
                Some(tx_id)
            );
        }
        assert_eq!(state.ready_queue.len(), state.ready_key_by_tx_id.len());
        assert_eq!(state.ready_by_seq.len(), state.ready_key_by_tx_id.len());
    });
    let (ok, indexed, expected) = evm_core::chain::verify_eth_tx_hash_index(u32::MAX);
    assert!(
        ok,
        "eth_tx_hash_index mismatch: indexed={indexed} expected={expected}"
    );
}

fn assert_no_pending_or_ready_refs(tx_id: TxId) {
    with_state(|state| {
        assert!(state.pending_meta_by_tx_id.get(&tx_id).is_none());
        assert!(state.pending_fee_key_by_tx_id.get(&tx_id).is_none());
        assert!(state.ready_key_by_tx_id.get(&tx_id).is_none());
        assert!(state
            .pending_by_sender_nonce
            .iter()
            .all(|entry| entry.value() != tx_id));
        assert!(state.ready_queue.iter().all(|entry| entry.value() != tx_id));
        assert!(state
            .ready_by_seq
            .iter()
            .all(|entry| entry.value() != tx_id));
        assert!(state
            .pending_fee_index
            .iter()
            .all(|entry| entry.value() != tx_id));
    });
}
