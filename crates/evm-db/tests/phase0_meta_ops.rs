//! どこで: Phase0テスト / 何を: Meta互換読込とOps storableの復元確認 / なぜ: upgrade時の退行を防ぐため

use evm_db::chain_data::{OpsConfigV1, OpsMode, OpsStateV1};
use evm_db::meta::{Meta, SchemaMigrationPhase, SchemaMigrationState};
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
