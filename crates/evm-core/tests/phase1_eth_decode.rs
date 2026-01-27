//! どこで: Phase1テスト / 何を: Eth raw tx デコードの失敗系 / なぜ: 仕様境界の確認

use evm_core::tx_decode::{decode_eth_raw_tx, DecodeError};

#[test]
fn eth_raw_typed_is_rejected() {
    let bytes = vec![0x02, 0x01, 0x02];
    let err = decode_eth_raw_tx(&bytes).err();
    assert_eq!(err, Some(DecodeError::UnsupportedType));
}

#[test]
fn eth_raw_empty_is_rejected() {
    let err = decode_eth_raw_tx(&[]).err();
    assert_eq!(err, Some(DecodeError::InvalidLength));
}
