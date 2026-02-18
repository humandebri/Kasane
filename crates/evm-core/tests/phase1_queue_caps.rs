//! どこで: Phase1テスト / 何を: mempoolのglobal cap拒否 / なぜ: 無限投入DoSを防ぐため

use alloy_consensus::{SignableTransaction, TxLegacy};
use alloy_eips::eip2718::Encodable2718;
use alloy_primitives::{Address, Bytes, Signature, TxKind as EthTxKind, U256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use evm_core::chain::{self, ChainError};
use evm_core::hash;
use evm_db::chain_data::constants::{
    CHAIN_ID, DROP_CODE_DECODE, DROP_CODE_EXEC_PRECHECK, DROP_CODE_REPLACED, MAX_PENDING_GLOBAL,
    MAX_PENDING_PER_PRINCIPAL,
};
use evm_db::chain_data::{
    CallerKey, SenderKey, SenderNonceKey, StoredTxBytes, TxId, TxKind, TxLocKind,
};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};

mod common;

fn relax_fee_floor_for_tests() {
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1;
        chain_state.min_gas_price = 1;
        chain_state.min_priority_fee = 1;
        state.chain_state.set(chain_state);
    });
}

#[test]
fn submit_ic_tx_rejects_when_global_pending_cap_is_reached() {
    init_stable_state();
    relax_fee_floor_for_tests();
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

    let err = chain::submit_ic_tx(vec![0x01], vec![0x02], common::build_default_ic_tx_bytes(0))
        .expect_err("global cap should reject submit");
    assert_eq!(err, ChainError::QueueFull);
}

#[test]
fn replacement_is_allowed_even_when_global_pending_cap_is_reached() {
    init_stable_state();
    relax_fee_floor_for_tests();
    let caller = vec![0x42];
    let canister = vec![0x77];
    let first_tx = common::build_default_ic_tx_bytes(0);
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

    let replacement_tx = common::build_ic_tx_bytes([0x10u8; 20], 0, 3_000_000_000, 2_000_000_000);
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
    relax_fee_floor_for_tests();
    let caller_low = vec![0x42];
    let caller_high = vec![0x43];
    let canister = vec![0x77];
    let low_fee_tx = common::build_default_ic_tx_bytes(0);
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
        common::build_ic_tx_bytes([0x10u8; 20], 0, 10_000_000_000, 5_000_000_000),
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
    relax_fee_floor_for_tests();
    let caller_low = vec![0x52];
    let caller_same = vec![0x53];
    let canister = vec![0x88];
    let _ = chain::submit_ic_tx(
        caller_low,
        canister.clone(),
        common::build_default_ic_tx_bytes(0),
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

    let err = chain::submit_ic_tx(caller_same, canister, common::build_default_ic_tx_bytes(0))
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
    relax_fee_floor_for_tests();
    let caller = vec![0x99];
    let canister = vec![0x01];
    let caller_evm = hash::derive_evm_address_from_principal(&caller).expect("must derive");
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
        let principal_key = CallerKey::from_principal_bytes(&caller);
        state
            .principal_pending_count
            .insert(principal_key, MAX_PENDING_PER_PRINCIPAL as u32);
    });

    let err = chain::submit_tx(TxKind::EthSigned, build_eth_signed_tx(0), caller)
        .expect_err("principal cap should reject submit");
    assert_eq!(err, ChainError::PrincipalQueueFull);
}

#[test]
fn eth_signed_submit_does_not_turn_into_decode_drop_on_produce() {
    init_stable_state();
    relax_fee_floor_for_tests();
    let gas_price = with_state(|state| state.chain_state.get().min_gas_price.saturating_add(1));
    let tx_id = chain::submit_tx(
        TxKind::EthSigned,
        build_eth_signed_tx_with_gas_price(0, u128::from(gas_price)),
        vec![0x42],
    )
    .expect("eth signed submit should succeed");

    let err = chain::produce_block(1).expect_err("insufficient sender balance should drop");
    assert_eq!(err, ChainError::NoExecutableTx);

    let loc = chain::get_tx_loc(&tx_id).expect("tx loc");
    assert_eq!(loc.kind, TxLocKind::Dropped);
    assert_eq!(loc.drop_code, DROP_CODE_EXEC_PRECHECK);
    assert_ne!(loc.drop_code, DROP_CODE_DECODE);
}

#[test]
fn submit_tx_nonce_conflict_is_atomic() {
    init_stable_state();
    relax_fee_floor_for_tests();
    let caller = vec![0x34];
    let gas_price = with_state(|state| state.chain_state.get().min_gas_price.saturating_add(1));
    let raw = build_eth_signed_tx_with_gas_price(0, u128::from(gas_price));
    let decoded = evm_core::tx_decode::decode_eth_raw_tx(&raw).expect("decode");
    let mut sender = [0u8; 20];
    sender.copy_from_slice(decoded.caller.as_ref());
    let pending_key = SenderNonceKey::new(sender, 0);
    let existing_tx_id = TxId([0x88u8; 32]);
    with_state_mut(|state| {
        state
            .sender_expected_nonce
            .insert(SenderKey::new(sender), 0);
        state
            .pending_by_sender_nonce
            .insert(pending_key, existing_tx_id);
        state
            .pending_meta_by_tx_id
            .insert(existing_tx_id, pending_key);
    });
    let new_tx_id = TxId(hash::stored_tx_id(
        TxKind::EthSigned,
        &raw,
        None,
        None,
        None,
    ));
    let err = chain::submit_tx(TxKind::EthSigned, raw.clone(), caller.clone())
        .expect_err("nonce conflict expected");
    assert_eq!(err, ChainError::NonceConflict);

    with_state(|state| {
        assert!(state.seen_tx.get(&new_tx_id).is_none());
        assert!(state.tx_store.get(&new_tx_id).is_none());
        assert!(chain::get_tx_loc(&new_tx_id).is_none());
        let eth_hash = TxId(hash::keccak256(&raw));
        assert!(state.eth_tx_hash_index.get(&eth_hash).is_none());
        assert_eq!(
            state
                .principal_pending_count
                .get(&CallerKey::from_principal_bytes(&caller)),
            None
        );
    });
}

#[test]
fn submit_ic_tx_nonce_conflict_is_atomic() {
    init_stable_state();
    relax_fee_floor_for_tests();
    let caller_principal = vec![0x45];
    let canister_id = vec![0x67];
    let sender = hash::derive_evm_address_from_principal(&caller_principal).expect("must derive");
    let pending_key = SenderNonceKey::new(sender, 0);
    let existing_tx_id = TxId([0x99u8; 32]);
    with_state_mut(|state| {
        state
            .sender_expected_nonce
            .insert(SenderKey::new(sender), 0);
        state
            .pending_by_sender_nonce
            .insert(pending_key, existing_tx_id);
        state
            .pending_meta_by_tx_id
            .insert(existing_tx_id, pending_key);
    });

    let tx_bytes = common::build_default_ic_tx_bytes(0);
    let new_tx_id = TxId(hash::stored_tx_id(
        TxKind::IcSynthetic,
        &tx_bytes,
        Some(sender),
        Some(&canister_id),
        Some(&caller_principal),
    ));
    let err = chain::submit_ic_tx(caller_principal.clone(), canister_id, tx_bytes)
        .expect_err("nonce conflict expected");
    assert_eq!(err, ChainError::NonceConflict);

    with_state(|state| {
        assert!(state.seen_tx.get(&new_tx_id).is_none());
        assert!(state.tx_store.get(&new_tx_id).is_none());
        assert!(chain::get_tx_loc(&new_tx_id).is_none());
        assert_eq!(
            state
                .principal_pending_count
                .get(&CallerKey::from_principal_bytes(&caller_principal)),
            None
        );
    });
}

fn build_eth_signed_tx(nonce: u64) -> Vec<u8> {
    build_eth_signed_tx_with_gas_price(nonce, 2_000_000_000)
}

fn build_eth_signed_tx_with_gas_price(nonce: u64, gas_price: u128) -> Vec<u8> {
    let signer: PrivateKeySigner =
        "0x59c6995e998f97a5a0044966f094538e0d7f4f4e4d5d8dd6a8c4f9d5f8b1e8a1"
            .parse()
            .expect("signer");
    let tx = TxLegacy {
        chain_id: Some(CHAIN_ID),
        nonce,
        gas_price,
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
