//! どこで: Phase1テスト / 何を: mempoolのglobal cap拒否 / なぜ: 無限投入DoSを防ぐため

use alloy_consensus::{SignableTransaction, TxLegacy};
use alloy_eips::eip2718::Encodable2718;
use alloy_primitives::{Address, Bytes, Signature, TxKind as EthTxKind, U256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use evm_core::chain::{self, ChainError};
use evm_core::hash;
use evm_db::chain_data::constants::{
    CHAIN_ID, DROP_CODE_REPLACED, MAX_PENDING_GLOBAL, MAX_PENDING_PER_PRINCIPAL,
};
use evm_db::chain_data::{SenderNonceKey, StoredTxBytes, TxId, TxKind, TxLocKind};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};

#[test]
fn submit_ic_tx_rejects_when_global_pending_cap_is_reached() {
    init_stable_state();
    with_state_mut(|state| {
        for i in 0..MAX_PENDING_GLOBAL {
            let mut sender = [0u8; 20];
            sender[18] = ((i >> 8) & 0xff) as u8;
            sender[19] = (i & 0xff) as u8;
            let key = SenderNonceKey::new(sender, 0);
            let mut tx_id = [0u8; 32];
            tx_id[28] = ((i >> 24) & 0xff) as u8;
            tx_id[29] = ((i >> 16) & 0xff) as u8;
            tx_id[30] = ((i >> 8) & 0xff) as u8;
            tx_id[31] = (i & 0xff) as u8;
            state.pending_by_sender_nonce.insert(key, TxId(tx_id));
        }
    });

    let err = chain::submit_ic_tx(
        vec![0x01],
        vec![0x02],
        build_ic_tx_bytes(0, 2_000_000_000, 1_000_000_000),
    )
    .expect_err("global cap should reject submit");
    assert_eq!(err, ChainError::QueueFull);
}

#[test]
fn replacement_is_allowed_even_when_global_pending_cap_is_reached() {
    init_stable_state();
    let caller = vec![0x42];
    let canister = vec![0x77];
    let first_tx = build_ic_tx_bytes(0, 2_000_000_000, 1_000_000_000);
    let first_tx_id =
        chain::submit_ic_tx(caller.clone(), canister.clone(), first_tx).expect("first submit");

    with_state_mut(|state| {
        // Keep the original sender entry, then fill up to the global cap with distinct senders.
        for i in 1..MAX_PENDING_GLOBAL {
            let mut sender = [0u8; 20];
            sender[18] = ((i >> 8) & 0xff) as u8;
            sender[19] = (i & 0xff) as u8;
            let key = SenderNonceKey::new(sender, 0);
            let mut tx_id = [0u8; 32];
            tx_id[28] = ((i >> 24) & 0xff) as u8;
            tx_id[29] = ((i >> 16) & 0xff) as u8;
            tx_id[30] = ((i >> 8) & 0xff) as u8;
            tx_id[31] = (i & 0xff) as u8;
            state.pending_by_sender_nonce.insert(key, TxId(tx_id));
        }
    });

    let replacement_tx = build_ic_tx_bytes(0, 3_000_000_000, 2_000_000_000);
    let replacement_tx_id = chain::submit_ic_tx(caller, canister, replacement_tx)
        .expect("replacement should be accepted");
    assert_ne!(replacement_tx_id, first_tx_id);
    let old_loc = chain::get_tx_loc(&first_tx_id).expect("old tx loc");
    assert_eq!(old_loc.kind, TxLocKind::Dropped);
    assert_eq!(old_loc.drop_code, DROP_CODE_REPLACED);
}

#[test]
fn higher_fee_tx_evicts_lowest_fee_when_global_pending_cap_is_reached() {
    init_stable_state();
    let caller_low = vec![0x42];
    let caller_high = vec![0x43];
    let canister = vec![0x77];
    let low_fee_tx = build_ic_tx_bytes(0, 2_000_000_000, 1_000_000_000);
    let low_fee_tx_id =
        chain::submit_ic_tx(caller_low, canister.clone(), low_fee_tx).expect("seed low fee tx");

    with_state_mut(|state| {
        for i in 1..MAX_PENDING_GLOBAL {
            let mut sender = [0u8; 20];
            sender[18] = ((i >> 8) & 0xff) as u8;
            sender[19] = (i & 0xff) as u8;
            let key = SenderNonceKey::new(sender, 0);
            let mut tx_id = [0u8; 32];
            tx_id[28] = ((i >> 24) & 0xff) as u8;
            tx_id[29] = ((i >> 16) & 0xff) as u8;
            tx_id[30] = ((i >> 8) & 0xff) as u8;
            tx_id[31] = (i & 0xff) as u8;
            state.pending_by_sender_nonce.insert(key, TxId(tx_id));
        }
    });

    let accepted = chain::submit_ic_tx(
        caller_high,
        canister,
        build_ic_tx_bytes(0, 10_000_000_000, 5_000_000_000),
    )
    .expect("higher fee tx should evict and be accepted");
    assert_ne!(accepted, low_fee_tx_id);
    let dropped = chain::get_tx_loc(&low_fee_tx_id).expect("evicted tx loc");
    assert_eq!(dropped.kind, TxLocKind::Dropped);
    assert_eq!(dropped.drop_code, DROP_CODE_REPLACED);
}

#[test]
fn lower_or_equal_fee_tx_is_rejected_when_global_pending_cap_is_reached() {
    init_stable_state();
    let caller_low = vec![0x52];
    let caller_same = vec![0x53];
    let canister = vec![0x88];
    let _ = chain::submit_ic_tx(
        caller_low,
        canister.clone(),
        build_ic_tx_bytes(0, 2_000_000_000, 1_000_000_000),
    )
    .expect("seed low fee tx");

    with_state_mut(|state| {
        for i in 1..MAX_PENDING_GLOBAL {
            let mut sender = [0u8; 20];
            sender[18] = ((i >> 8) & 0xff) as u8;
            sender[19] = (i & 0xff) as u8;
            let key = SenderNonceKey::new(sender, 0);
            let mut tx_id = [0u8; 32];
            tx_id[28] = ((i >> 24) & 0xff) as u8;
            tx_id[29] = ((i >> 16) & 0xff) as u8;
            tx_id[30] = ((i >> 8) & 0xff) as u8;
            tx_id[31] = (i & 0xff) as u8;
            state.pending_by_sender_nonce.insert(key, TxId(tx_id));
        }
    });

    let err = chain::submit_ic_tx(
        caller_same,
        canister,
        build_ic_tx_bytes(0, 2_000_000_000, 1_000_000_000),
    )
    .expect_err("same fee should be rejected under full global cap");
    assert_eq!(err, ChainError::QueueFull);
    with_state(|state| {
        assert_eq!(
            state.pending_by_sender_nonce.len(),
            MAX_PENDING_GLOBAL as u64
        );
    });
}

#[test]
fn submit_ic_tx_rejects_when_principal_pending_cap_is_reached() {
    init_stable_state();
    let caller = vec![0x99];
    let canister = vec![0x01];
    let caller_evm = hash::caller_evm_from_principal(&caller);
    with_state_mut(|state| {
        for i in 0..MAX_PENDING_PER_PRINCIPAL {
            let mut sender = [0u8; 20];
            sender[18] = ((i >> 8) & 0xff) as u8;
            sender[19] = (i & 0xff) as u8;
            let pending_key = SenderNonceKey::new(sender, 0);
            let raw = vec![i as u8, 0xaa, 0xbb];
            let tx_id = TxId(hash::stored_tx_id(
                TxKind::IcSynthetic,
                &raw,
                Some(caller_evm),
                Some(&canister),
                Some(&caller),
            ));
            let envelope = StoredTxBytes::new_with_fees(
                tx_id,
                TxKind::IcSynthetic,
                raw,
                Some(caller_evm),
                canister.clone(),
                caller.clone(),
                2_000_000_000,
                1_000_000_000,
                true,
            );
            state.pending_by_sender_nonce.insert(pending_key, tx_id);
            state.tx_store.insert(tx_id, envelope);
        }
    });

    let err = chain::submit_tx(TxKind::EthSigned, build_eth_signed_tx(0), caller)
        .expect_err("principal cap should reject submit");
    assert_eq!(err, ChainError::PrincipalQueueFull);
}

fn build_eth_signed_tx(nonce: u64) -> Vec<u8> {
    let signer: PrivateKeySigner =
        "0x59c6995e998f97a5a0044966f094538e0d7f4f4e4d5d8dd6a8c4f9d5f8b1e8a1"
            .parse()
            .expect("signer");
    let tx = TxLegacy {
        chain_id: Some(CHAIN_ID),
        nonce,
        gas_price: 2_000_000_000,
        gas_limit: 21_000,
        to: EthTxKind::Call(Address::from([0x11u8; 20])),
        value: U256::ZERO,
        input: Bytes::new(),
    };
    sign_encoded(tx, &signer)
}

fn sign_encoded<T>(tx: T, signer: &PrivateKeySigner) -> Vec<u8>
where
    T: alloy_consensus::transaction::RlpEcdsaEncodableTx
        + alloy_eips::Typed2718
        + SignableTransaction<Signature>
        + Send
        + Sync,
{
    let hash = tx.signature_hash();
    let signature = signer.sign_hash_sync(&hash).expect("sign");
    let signed = tx.into_signed(signature);
    signed.encoded_2718()
}

fn build_ic_tx_bytes(nonce: u64, max_fee_per_gas: u128, max_priority_fee_per_gas: u128) -> Vec<u8> {
    let to = [0x10u8; 20];
    let value = [0u8; 32];
    let gas_limit = 50_000u64.to_be_bytes();
    let nonce = nonce.to_be_bytes();
    let max_fee = max_fee_per_gas.to_be_bytes();
    let max_priority = max_priority_fee_per_gas.to_be_bytes();
    let data: Vec<u8> = Vec::new();
    let data_len = 0u32.to_be_bytes();
    let mut out = Vec::new();
    out.push(2u8);
    out.extend_from_slice(&to);
    out.extend_from_slice(&value);
    out.extend_from_slice(&gas_limit);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&max_fee);
    out.extend_from_slice(&max_priority);
    out.extend_from_slice(&data_len);
    out.extend_from_slice(&data);
    out
}
