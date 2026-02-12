//! どこで: Phase0テスト / 何を: Meta互換読込とOps storableの復元確認 / なぜ: upgrade時の退行を防ぐため

use evm_db::chain_data::{CallerKey, OpsConfigV1, OpsMode, OpsStateV1, TxLoc};
use evm_db::meta::{needs_migration, Meta, SchemaMigrationPhase, SchemaMigrationState};
use evm_db::stable_state::init_stable_state;
use ic_stable_structures::Storable;
use std::borrow::Cow;

#[test]
fn meta_reads_legacy_payload_as_needs_migration() {
    let mut legacy = [0u8; 40];
    legacy[0..4].copy_from_slice(b"EVM0");
    legacy[4..8].copy_from_slice(&1u32.to_be_bytes());
    let meta = Meta::from_bytes(Cow::Borrowed(&legacy));
    assert_eq!(meta.schema_version, 1);
    assert!(meta.needs_migration);
}

#[test]
fn meta_roundtrip_keeps_new_fields() {
    let mut meta = Meta::new();
    meta.needs_migration = true;
    meta.last_migration_from = 1;
    meta.last_migration_to = 2;
    meta.last_migration_ts = 9;
    let encoded = meta.to_bytes().into_owned();
    assert_eq!(encoded.len(), 64);
    let decoded = Meta::from_bytes(Cow::Owned(encoded));
    assert_eq!(decoded, meta);
}

#[test]
fn fail_closed_decode_sets_needs_migration() {
    init_stable_state();
    assert!(!needs_migration());
    let _ = TxLoc::from_bytes(Cow::Borrowed(&[0u8; 1]));
    assert!(needs_migration());
}

#[test]
fn meta_invalid_length_is_fail_closed_for_non_legacy_sizes() {
    for len in [1usize, 63usize] {
        let meta = Meta::from_bytes(Cow::Owned(vec![0u8; len]));
        assert!(meta.needs_migration);
        assert_eq!(meta.schema_version, 0);
    }
}

#[test]
fn ops_state_roundtrip() {
    let state = OpsStateV1 {
        last_cycle_balance: 7,
        last_check_ts: 8,
        mode: OpsMode::Critical,
        safe_stop_latched: true,
    };
    let decoded = OpsStateV1::from_bytes(Cow::Owned(state.to_bytes().into_owned()));
    assert_eq!(decoded, state);
}

#[test]
fn ops_config_roundtrip() {
    let config = OpsConfigV1 {
        low_watermark: 10,
        critical: 3,
        freeze_on_critical: false,
    };
    let decoded = OpsConfigV1::from_bytes(Cow::Owned(config.to_bytes().into_owned()));
    assert_eq!(decoded, config);
}

#[test]
fn ops_state_invalid_len_sets_needs_migration() {
    init_stable_state();
    assert!(!needs_migration());
    let decoded = OpsStateV1::from_bytes(Cow::Owned(vec![0u8; 1]));
    assert_eq!(decoded.mode, OpsMode::Critical);
    assert!(decoded.safe_stop_latched);
    assert!(needs_migration());
}

#[test]
fn ops_config_invalid_len_sets_needs_migration() {
    init_stable_state();
    assert!(!needs_migration());
    let decoded = OpsConfigV1::from_bytes(Cow::Owned(vec![0u8; 1]));
    assert_eq!(decoded.critical, u128::MAX);
    assert_eq!(decoded.low_watermark, u128::MAX);
    assert!(needs_migration());
}

#[test]
fn caller_key_invalid_len_sets_needs_migration() {
    init_stable_state();
    assert!(!needs_migration());
    let decoded = CallerKey::from_bytes(Cow::Owned(vec![0u8; 1]));
    assert_eq!(decoded.0, [0u8; 30]);
    assert!(needs_migration());
}

#[test]
fn schema_migration_state_roundtrip_with_cursor_key() {
    let mut state = SchemaMigrationState::done();
    state.phase = SchemaMigrationPhase::Rewrite;
    state.cursor = 12;
    state.from_version = 1;
    state.to_version = 3;
    state.last_error = 9;
    state.cursor_key_set = true;
    state.cursor_key = [0xabu8; 32];
    let decoded = SchemaMigrationState::from_bytes(Cow::Owned(state.to_bytes().into_owned()));
    assert_eq!(decoded, state);
}
