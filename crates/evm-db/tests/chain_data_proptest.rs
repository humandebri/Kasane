//! どこで: evm-db codec性質テスト / 何を: roundtripと境界条件を乱択検証 / なぜ: 破損時の誤検知と見逃しを減らすため

use evm_db::chain_data::{TxLoc, TxLocKind};
use evm_db::types::keys::{
    make_account_key, make_storage_key, parse_account_key_bytes, parse_storage_key_bytes,
};
use ic_stable_structures::Storable;
use proptest::prelude::*;

fn tx_loc_strategy() -> impl Strategy<Value = TxLoc> {
    prop_oneof![
        any::<u64>().prop_map(TxLoc::queued),
        (any::<u64>(), any::<u32>()).prop_map(|(block, idx)| TxLoc::included(block, idx)),
        any::<u16>().prop_map(TxLoc::dropped),
    ]
}

proptest! {
    #[test]
    fn tx_loc_roundtrip_property(loc in tx_loc_strategy()) {
        let encoded = loc.to_bytes();
        let decoded = TxLoc::from_bytes(encoded);
        prop_assert_eq!(decoded, loc);
    }

    #[test]
    fn tx_loc_decode_never_panics_for_any_input(bytes in proptest::collection::vec(any::<u8>(), 0..128)) {
        let _ = TxLoc::from_bytes(std::borrow::Cow::Owned(bytes));
    }

    #[test]
    fn account_key_wire_roundtrip_property(addr in any::<[u8;20]>()) {
        let key = make_account_key(addr);
        let decoded = parse_account_key_bytes(&key.0);
        prop_assert_eq!(decoded, Some(addr));
    }

    #[test]
    fn storage_key_wire_roundtrip_property(addr in any::<[u8;20]>(), slot in any::<[u8;32]>()) {
        let key = make_storage_key(addr, slot);
        let decoded = parse_storage_key_bytes(&key.0);
        prop_assert_eq!(decoded, Some((addr, slot)));
    }

    #[test]
    fn storage_key_wire_rejects_invalid_prefix(mut addr in any::<[u8;20]>(), slot in any::<[u8;32]>()) {
        // prefixだけを壊してもpanicせずNoneになることを保証する。
        addr[0] = addr[0].wrapping_add(1);
        let mut raw = make_storage_key(addr, slot).0;
        raw[0] = 0xff;
        let decoded = parse_storage_key_bytes(&raw);
        prop_assert!(decoded.is_none());
    }
}

#[test]
fn tx_loc_legacy_decode_path_is_supported() {
    let mut legacy = [0u8; 24];
    legacy[0] = 2;
    legacy[21..23].copy_from_slice(&77u16.to_be_bytes());
    let decoded = TxLoc::from_bytes(std::borrow::Cow::Owned(legacy.to_vec()));
    assert_eq!(decoded.kind, TxLocKind::Dropped);
    assert_eq!(decoded.drop_code, 77);
}
