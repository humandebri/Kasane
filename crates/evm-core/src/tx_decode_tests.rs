use super::{decode_eth_raw_tx, should_reject_unsupported_typed_tx, DecodeError};

#[test]
fn unsupported_typed_tx_prefixes_are_rejected_early() {
    assert!(should_reject_unsupported_typed_tx(0x03));
    assert!(should_reject_unsupported_typed_tx(0x04));
    assert!(!should_reject_unsupported_typed_tx(0x01));
    assert!(!should_reject_unsupported_typed_tx(0x02));
}

#[test]
fn decode_eth_raw_tx_rejects_4844_and_7702_prefix_without_deep_decode() {
    assert_eq!(
        decode_eth_raw_tx(&[0x03]).err(),
        Some(DecodeError::UnsupportedType)
    );
    assert_eq!(
        decode_eth_raw_tx(&[0x04]).err(),
        Some(DecodeError::UnsupportedType)
    );
}

#[test]
fn decode_eth_raw_tx_rejects_legacy_typed_0x00_prefix() {
    assert_eq!(
        decode_eth_raw_tx(&[0x00, 0xc0]).err(),
        Some(DecodeError::UnsupportedType)
    );
}
