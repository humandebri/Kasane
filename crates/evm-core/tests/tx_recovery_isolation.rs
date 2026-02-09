//! どこで: evm-core の recovery境界テスト
//! 何を: tx_decode 公開API経由で recovery隔離後の挙動を固定
//! なぜ: tx_recovery 封じ込めで仕様回帰を防ぐため

use alloy_consensus::{SignableTransaction, TxEip1559, TxLegacy};
use alloy_eips::eip2718::Encodable2718;
use alloy_primitives::{Address, Bytes, Signature, TxKind, U256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use evm_core::tx_decode::{decode_eth_raw_tx, DecodeError};
use evm_db::chain_data::constants::CHAIN_ID;

#[test]
fn recovery_path_accepts_legacy_and_returns_sender() {
    let signer = test_signer();
    let raw = sign_encoded(tx_legacy(Some(CHAIN_ID), 0), &signer);
    let decoded = decode_eth_raw_tx(&raw).expect("legacy tx should decode");
    assert_eq!(decoded.caller, signer.address());
    assert_eq!(decoded.chain_id, Some(CHAIN_ID));
}

#[test]
fn recovery_path_checks_chain_id_before_signature() {
    let raw = encode_with_signature(tx_1559(CHAIN_ID + 1, 1), bad_signature());
    let err = decode_eth_raw_tx(&raw).expect_err("wrong chain id must fail first");
    assert_eq!(err, DecodeError::WrongChainId);
}

#[test]
fn recovery_path_rejects_unsupported_typed_prefix_early() {
    let err = decode_eth_raw_tx(&[0x03]).expect_err("4844 prefix must be rejected early");
    assert_eq!(err, DecodeError::UnsupportedType);
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
    tx.into_signed(signature).encoded_2718()
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

fn tx_1559(chain_id: u64, nonce: u64) -> TxEip1559 {
    TxEip1559 {
        chain_id,
        nonce,
        gas_limit: 40_000,
        max_fee_per_gas: 5,
        max_priority_fee_per_gas: 1,
        to: TxKind::Call(Address::from([0x13u8; 20])),
        value: U256::ZERO,
        access_list: Default::default(),
        input: Bytes::new(),
    }
}

