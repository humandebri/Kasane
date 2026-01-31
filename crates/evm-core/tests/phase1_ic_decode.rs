//! どこで: Phase1テスト / 何を: IcSyntheticデコード / なぜ: 仕様固定のため

use evm_core::tx_decode::{decode_ic_synthetic, DecodeError};
use revm::primitives::{address, U256};

#[test]
fn decode_ic_tx_roundtrip() {
    let caller = address!("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let to = [0x11u8; 20];
    let value = [0x22u8; 32];
    let gas = 21_000u64.to_be_bytes();
    let nonce = 7u64.to_be_bytes();
    let max_fee = 30u128.to_be_bytes();
    let max_priority = 2u128.to_be_bytes();
    let data = vec![1, 2, 3];
    let data_len = (data.len() as u32).to_be_bytes();

    let mut bytes = Vec::new();
    bytes.push(2u8);
    bytes.extend_from_slice(&to);
    bytes.extend_from_slice(&value);
    bytes.extend_from_slice(&gas);
    bytes.extend_from_slice(&nonce);
    bytes.extend_from_slice(&max_fee);
    bytes.extend_from_slice(&max_priority);
    bytes.extend_from_slice(&data_len);
    bytes.extend_from_slice(&data);

    let tx = decode_ic_synthetic(caller, &bytes).expect("decode");
    assert_eq!(tx.caller, caller);
    assert_eq!(tx.value, U256::from_be_bytes(value));
    assert_eq!(tx.gas_limit, 21_000);
    assert_eq!(tx.nonce, 7);
    assert_eq!(tx.data.as_ref(), data.as_slice());
    assert_eq!(tx.gas_price, 30);
    assert_eq!(tx.gas_priority_fee, Some(2));
}

#[test]
fn decode_ic_tx_rejects_version() {
    let caller = address!("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    let mut bytes = vec![0u8; 1 + 20 + 32 + 8 + 8 + 16 + 16 + 4];
    bytes[0] = 1;
    let err = decode_ic_synthetic(caller, &bytes).err();
    assert_eq!(err, Some(DecodeError::InvalidVersion));
}
