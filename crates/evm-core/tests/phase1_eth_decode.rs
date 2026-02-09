//! どこで: Phase1テスト / 何を: Eth raw tx デコード / なぜ: 分類と仕様境界を固定するため

use alloy_consensus::{SignableTransaction, TxEip1559, TxEip2930, TxEip4844, TxEip7702, TxLegacy};
use alloy_eips::eip2718::Encodable2718;
use alloy_eips::eip2930::AccessList;
use alloy_eips::eip7702::Authorization;
use alloy_primitives::{Address, Bytes, Signature, TxKind, B256, U256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use evm_core::chain::{self, ChainError};
use evm_core::tx_decode::{decode_eth_raw_tx, DecodeError};
use evm_db::chain_data::TxKind as StoredTxKind;
use evm_db::chain_data::constants::CHAIN_ID;
use evm_db::stable_state::{init_stable_state, with_state_mut};

#[test]
fn eth_raw_typed_invalid_is_rejected() {
    let bytes = vec![0x02, 0x01, 0x02];
    let err = decode_eth_raw_tx(&bytes).err();
    assert_eq!(err, Some(DecodeError::InvalidRlp));
}

#[test]
fn eth_raw_empty_is_rejected() {
    let err = decode_eth_raw_tx(&[]).err();
    assert_eq!(err, Some(DecodeError::InvalidLength));
}

#[test]
fn eth_raw_trailing_bytes_is_rejected() {
    let signer = test_signer();
    let tx = tx_1559(CHAIN_ID, 0);
    let mut bytes = sign_encoded(tx, &signer);
    bytes.push(0u8);
    let err = decode_eth_raw_tx(&bytes).err();
    assert_eq!(err, Some(DecodeError::TrailingBytes));
}

#[test]
fn decode_accepts_legacy_2930_1559() {
    let signer = test_signer();
    let expected = signer.address();

    let legacy = decode_eth_raw_tx(&sign_encoded(tx_legacy(Some(CHAIN_ID), 0), &signer)).unwrap();
    assert_eq!(legacy.caller, expected);
    assert_eq!(legacy.chain_id, Some(CHAIN_ID));

    let eip2930 = decode_eth_raw_tx(&sign_encoded(tx_2930(CHAIN_ID, 1), &signer)).unwrap();
    assert_eq!(eip2930.caller, expected);
    assert_eq!(eip2930.tx_type, 1);

    let eip1559 = decode_eth_raw_tx(&sign_encoded(tx_1559(CHAIN_ID, 2), &signer)).unwrap();
    assert_eq!(eip1559.caller, expected);
    assert_eq!(eip1559.tx_type, 2);
}

#[test]
fn decode_rejects_4844_and_7702_early() {
    let signer = test_signer();
    let eip4844 = sign_encoded(tx_4844(CHAIN_ID, 3), &signer);
    let eip7702 = sign_encoded(tx_7702(CHAIN_ID, 4, &signer), &signer);
    assert_eq!(decode_eth_raw_tx(&eip4844).err(), Some(DecodeError::UnsupportedType));
    assert_eq!(decode_eth_raw_tx(&eip7702).err(), Some(DecodeError::UnsupportedType));
}

#[test]
fn submit_rejects_eip4844_eth_tx_early() {
    init_stable_state();
    set_test_fee_policy();

    let signer = test_signer();
    let raw = sign_encoded(tx_4844_executable(CHAIN_ID, 0), &signer);

    let err = chain::submit_tx(StoredTxKind::EthSigned, raw, vec![0x48]).expect_err("submit 4844");
    assert_eq!(err, ChainError::DecodeFailed);
}

#[test]
fn submit_rejects_eip7702_eth_tx_early() {
    init_stable_state();
    set_test_fee_policy();

    let signer = test_signer();
    let raw = sign_encoded(tx_7702_executable(CHAIN_ID, 0, &signer), &signer);

    let err = chain::submit_tx(StoredTxKind::EthSigned, raw, vec![0x77]).expect_err("submit 7702");
    assert_eq!(err, ChainError::DecodeFailed);
}

#[test]
fn chain_id_none_is_rejected_before_signature() {
    let signer = test_signer();
    let tx = tx_legacy(None, 10);
    let bytes = sign_encoded(tx, &signer);
    let err = decode_eth_raw_tx(&bytes).err();
    assert_eq!(err, Some(DecodeError::LegacyChainIdMissing));
}

#[test]
fn wrong_chain_id_wins_even_with_invalid_signature() {
    let tx = tx_1559(CHAIN_ID + 1, 11);
    let bytes = encode_with_signature(tx, bad_signature());
    let err = decode_eth_raw_tx(&bytes).err();
    assert_eq!(err, Some(DecodeError::WrongChainId));
}

#[test]
fn invalid_signature_after_chain_id_passes_strict_check() {
    let tx = tx_1559(CHAIN_ID, 12);
    let bytes = encode_with_signature(tx, bad_signature());
    let err = decode_eth_raw_tx(&bytes).err();
    assert_eq!(err, Some(DecodeError::InvalidSignature));
}

fn test_signer() -> PrivateKeySigner {
    "0x59c6995e998f97a5a0044966f094538e0d7f4f4e4d5d8dd6a8c4f9d5f8b1e8a1"
        .parse()
        .expect("signer")
}

fn bad_signature() -> Signature {
    Signature::new(U256::from(1u64), U256::MAX, false)
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
    encode_with_signature(tx, signature)
}

fn encode_with_signature<T>(tx: T, signature: Signature) -> Vec<u8>
where
    T: alloy_consensus::transaction::RlpEcdsaEncodableTx
        + alloy_eips::Typed2718
        + SignableTransaction<Signature>
        + Send
        + Sync,
{
    let signed = tx.into_signed(signature);
    signed.encoded_2718()
}

fn tx_legacy(chain_id: Option<u64>, nonce: u64) -> TxLegacy {
    TxLegacy {
        chain_id,
        nonce,
        gas_price: 2,
        gas_limit: 21_000,
        to: TxKind::Call(Address::from([0x11u8; 20])),
        value: U256::ZERO,
        input: Bytes::new(),
    }
}

fn tx_2930(chain_id: u64, nonce: u64) -> TxEip2930 {
    TxEip2930 {
        chain_id,
        nonce,
        gas_price: 3,
        gas_limit: 30_000,
        to: TxKind::Call(Address::from([0x12u8; 20])),
        value: U256::ZERO,
        access_list: AccessList::default(),
        input: Bytes::new(),
    }
}

fn tx_1559(chain_id: u64, nonce: u64) -> TxEip1559 {
    TxEip1559 {
        chain_id,
        nonce,
        gas_limit: 40_000,
        max_fee_per_gas: 5,
        max_priority_fee_per_gas: 1,
        to: TxKind::Call(Address::from([0x13u8; 20])),
        value: U256::ZERO,
        access_list: AccessList::default(),
        input: Bytes::new(),
    }
}

fn tx_4844(chain_id: u64, nonce: u64) -> TxEip4844 {
    TxEip4844 {
        chain_id,
        nonce,
        gas_limit: 50_000,
        max_fee_per_gas: 6,
        max_priority_fee_per_gas: 1,
        to: Address::from([0x14u8; 20]),
        value: U256::ZERO,
        access_list: AccessList::default(),
        blob_versioned_hashes: vec![B256::from([0x44u8; 32])],
        max_fee_per_blob_gas: 4,
        input: Bytes::new(),
    }
}

fn tx_7702(chain_id: u64, nonce: u64, signer: &PrivateKeySigner) -> TxEip7702 {
    let auth = Authorization {
        chain_id: U256::from(chain_id),
        address: Address::from([0x15u8; 20]),
        nonce,
    };
    let auth_sig = signer
        .sign_hash_sync(&auth.signature_hash())
        .expect("sign auth");
    TxEip7702 {
        chain_id,
        nonce,
        gas_limit: 60_000,
        max_fee_per_gas: 7,
        max_priority_fee_per_gas: 2,
        to: Address::from([0x16u8; 20]),
        value: U256::ZERO,
        access_list: AccessList::default(),
        authorization_list: vec![auth.into_signed(auth_sig)],
        input: Bytes::new(),
    }
}

fn tx_4844_executable(chain_id: u64, nonce: u64) -> TxEip4844 {
    TxEip4844 {
        max_fee_per_gas: 2_000_000_000,
        max_priority_fee_per_gas: 1_000_000_000,
        ..tx_4844(chain_id, nonce)
    }
}

fn tx_7702_executable(chain_id: u64, nonce: u64, signer: &PrivateKeySigner) -> TxEip7702 {
    TxEip7702 {
        max_fee_per_gas: 2_000_000_000,
        max_priority_fee_per_gas: 1_000_000_000,
        ..tx_7702(chain_id, nonce, signer)
    }
}

fn set_test_fee_policy() {
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 1_000_000_000;
        chain_state.min_gas_price = 0;
        state.chain_state.set(chain_state);
    });
}
