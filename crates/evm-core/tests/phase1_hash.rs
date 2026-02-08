//! どこで: Phase1テスト / 何を: ハッシュ決定性 / なぜ: 再現性を保証するため

use evm_core::chain;
use evm_core::hash::{block_hash, keccak256, stored_tx_id, tx_list_hash};
use evm_core::state_root::{
    commit_state_root_with, compute_state_root_incremental_with, TouchedSummary,
};
use evm_db::chain_data::TxKind;
use evm_db::stable_state::{init_stable_state, with_state_mut};
use evm_db::types::keys::{make_account_key, make_storage_key};
use evm_db::types::values::{AccountVal, U256Val};

#[test]
fn tx_id_is_deterministic() {
    let a = stored_tx_id(TxKind::EthSigned, b"hello", None, None, None);
    let b = stored_tx_id(TxKind::EthSigned, b"hello", None, None, None);
    assert_eq!(a, b);
}

#[test]
fn tx_list_hash_depends_on_order() {
    let a = stored_tx_id(TxKind::EthSigned, b"a", None, None, None);
    let b = stored_tx_id(TxKind::EthSigned, b"b", None, None, None);
    let list1 = tx_list_hash(&[a, b]);
    let list2 = tx_list_hash(&[b, a]);
    assert_ne!(list1, list2);
}

#[test]
fn block_hash_is_deterministic() {
    let parent = keccak256(b"parent");
    let tx_list = keccak256(b"txs");
    let state_root = keccak256(b"state");
    let h1 = block_hash(parent, 1, 1, tx_list, state_root);
    let h2 = block_hash(parent, 1, 1, tx_list, state_root);
    assert_eq!(h1, h2);
}

#[test]
fn empty_state_root_matches_ethereum_empty_trie() {
    init_stable_state();
    let root = with_state_mut(|state| compute_state_root_incremental_with(state, &[]));
    assert_eq!(
        hex32(root),
        "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
    );
}

#[test]
fn state_root_is_deterministic_for_same_state() {
    init_stable_state();
    let addr = [0x11u8; 20];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr),
            AccountVal::from_parts(1, [0u8; 32], [0u8; 32]),
        );
        state.storage.insert(
            make_storage_key(addr, [0x01u8; 32]),
            U256Val::new([0x0au8; 32]),
        );
    });
    let root_a = with_state_mut(|state| compute_state_root_incremental_with(state, &[addr]));
    let root_b = with_state_mut(|state| compute_state_root_incremental_with(state, &[addr]));
    assert_eq!(root_a, root_b);
}

#[test]
fn state_root_is_stable_against_storage_insertion_order() {
    init_stable_state();
    let addr = [0x22u8; 20];
    let slot_a = [0x01u8; 32];
    let slot_b = [0x02u8; 32];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr),
            AccountVal::from_parts(1, [0u8; 32], [0u8; 32]),
        );
        state
            .storage
            .insert(make_storage_key(addr, slot_a), U256Val::new([0x0au8; 32]));
        state
            .storage
            .insert(make_storage_key(addr, slot_b), U256Val::new([0x0bu8; 32]));
    });
    let root_a = with_state_mut(|state| compute_state_root_incremental_with(state, &[addr]));
    with_state_mut(|state| {
        state.storage.remove(&make_storage_key(addr, slot_a));
        state.storage.remove(&make_storage_key(addr, slot_b));
        state
            .storage
            .insert(make_storage_key(addr, slot_b), U256Val::new([0x0bu8; 32]));
        state
            .storage
            .insert(make_storage_key(addr, slot_a), U256Val::new([0x0au8; 32]));
    });
    let root_b = with_state_mut(|state| compute_state_root_incremental_with(state, &[addr]));
    assert_eq!(root_a, root_b);
}

#[test]
fn state_root_changes_when_storage_changes() {
    init_stable_state();
    let addr = [0x33u8; 20];
    let slot = [0x03u8; 32];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr),
            AccountVal::from_parts(1, [0u8; 32], [0u8; 32]),
        );
        state
            .storage
            .insert(make_storage_key(addr, slot), U256Val::new([0x0au8; 32]));
    });
    let root_a = with_state_mut(|state| compute_state_root_incremental_with(state, &[addr]));
    with_state_mut(|state| {
        state
            .storage
            .insert(make_storage_key(addr, slot), U256Val::new([0x0cu8; 32]));
    });
    let root_b = with_state_mut(|state| compute_state_root_incremental_with(state, &[addr]));
    assert_ne!(root_a, root_b);
}

#[test]
fn incremental_state_root_matches_repeated_commit() {
    init_stable_state();
    let addr_a = [0x55u8; 20];
    let addr_b = [0x66u8; 20];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr_a),
            AccountVal::from_parts(1, [0x01u8; 32], [0u8; 32]),
        );
        state.accounts.insert(
            make_account_key(addr_b),
            AccountVal::from_parts(2, [0x02u8; 32], [0u8; 32]),
        );
        state.storage.insert(
            make_storage_key(addr_a, [0x01u8; 32]),
            U256Val::new([0x0au8; 32]),
        );
        state.storage.insert(
            make_storage_key(addr_b, [0x02u8; 32]),
            U256Val::new([0x0bu8; 32]),
        );
    });
    let incremental =
        with_state_mut(|state| compute_state_root_incremental_with(state, &[addr_a, addr_b]));
    let repeated =
        with_state_mut(|state| compute_state_root_incremental_with(state, &[addr_a, addr_b]));
    assert_eq!(incremental, repeated);
}

#[test]
fn state_root_sampling_verify_and_skip_counters_update() {
    init_stable_state();
    let addr = [0x77u8; 20];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr),
            AccountVal::from_parts(1, [0x01u8; 32], [0u8; 32]),
        );
    });
    with_state_mut(|state| {
        let summary = TouchedSummary {
            accounts_count: 1,
            slots_count: 0,
            delta_digest: [0u8; 32],
        };
        let _ =
            commit_state_root_with(state, &[addr], summary, 1, [0u8; 32], 1).expect("commit root");
        let metrics = *state.state_root_metrics.get();
        assert_eq!(metrics.state_root_verify_count, 1);
    });
    with_state_mut(|state| {
        let summary = TouchedSummary {
            accounts_count: 100,
            slots_count: 1000,
            delta_digest: [1u8; 32],
        };
        let _ =
            commit_state_root_with(state, &[addr], summary, 2, [0u8; 32], 2).expect("commit root");
        let metrics = *state.state_root_metrics.get();
        assert_eq!(metrics.state_root_verify_skipped_count, 1);
    });
}

#[test]
fn state_root_commit_does_not_fail_without_reference_verify() {
    init_stable_state();
    let addr_a = [0x81u8; 20];
    let addr_b = [0x82u8; 20];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr_a),
            AccountVal::from_parts(1, [0u8; 32], [0u8; 32]),
        );
        state.storage.insert(
            make_storage_key(addr_a, [0x01u8; 32]),
            U256Val::new([0x11u8; 32]),
        );
        let summary = TouchedSummary {
            accounts_count: 1,
            slots_count: 1,
            delta_digest: [0u8; 32],
        };
        let _ = commit_state_root_with(state, &[addr_a], summary, 1, [0x22u8; 32], 11)
            .expect("first commit");
    });
    with_state_mut(|state| {
        let before_meta = *state.state_root_meta.get();
        let before_root_cache = state.state_storage_roots.len();
        state.accounts.insert(
            make_account_key(addr_b),
            AccountVal::from_parts(1, [0u8; 32], [0u8; 32]),
        );
        state.storage.insert(
            make_storage_key(addr_b, [0x02u8; 32]),
            U256Val::new([0x22u8; 32]),
        );
        let summary = TouchedSummary {
            accounts_count: 1,
            slots_count: 1,
            delta_digest: [0x33u8; 32],
        };
        let out = commit_state_root_with(state, &[addr_b], summary, 2, [0x44u8; 32], 12)
            .expect("commit should not fail without reference verify");
        assert_ne!(out, before_meta.state_root);
        let metrics = *state.state_root_metrics.get();
        assert_eq!(metrics.state_root_mismatch_count, 0);
        assert_ne!(*state.state_root_meta.get(), before_meta);
        assert!(state.state_storage_roots.len() >= before_root_cache);
        assert!(state.state_root_mismatch.get(&2).is_none());
    });
}

#[test]
fn node_db_metrics_are_updated() {
    init_stable_state();
    let addr = [0x91u8; 20];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr),
            AccountVal::from_parts(1, [0u8; 32], [0u8; 32]),
        );
        for i in 0..130u64 {
            state.accounts.insert(
                make_account_key(addr),
                AccountVal::from_parts(i + 1, [0u8; 32], [0u8; 32]),
            );
            let summary = TouchedSummary {
                accounts_count: 1,
                slots_count: 0,
                delta_digest: [u8::try_from(i & 0xff).unwrap_or(0); 32],
            };
            let _ = commit_state_root_with(state, &[addr], summary, i + 1, [0u8; 32], i + 1)
                .expect("commit must succeed");
        }
        let metrics = *state.state_root_metrics.get();
        assert!(metrics.node_db_entries > 0);
        assert!(metrics.node_db_reachable > 0);
        assert_eq!(metrics.node_db_unreachable, 0);
    });
}

#[test]
fn zero_code_hash_account_is_treated_as_empty_code_hash() {
    init_stable_state();
    let addr = [0x44u8; 20];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr),
            AccountVal::from_parts(0, [0u8; 32], [0u8; 32]),
        );
    });
    let root = with_state_mut(|state| compute_state_root_incremental_with(state, &[addr]));
    assert_eq!(
        hex32(root),
        "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
    );
}

#[test]
fn commit_state_root_noop_fast_path_uses_current_root() {
    init_stable_state();
    with_state_mut(|state| {
        let baseline = commit_state_root_with(
            state,
            &[],
            TouchedSummary {
                accounts_count: 1,
                slots_count: 1,
                delta_digest: [1u8; 32],
            },
            1,
            [0u8; 32],
            1,
        )
        .expect("baseline commit");
        let before = *state.state_root_metrics.get();
        let root = commit_state_root_with(
            state,
            &[],
            TouchedSummary {
                accounts_count: 0,
                slots_count: 0,
                delta_digest: [0u8; 32],
            },
            2,
            [0u8; 32],
            2,
        )
        .expect("noop fast-path commit");
        assert_eq!(root, baseline);
        let after = *state.state_root_metrics.get();
        assert_eq!(
            after.state_root_verify_count,
            before.state_root_verify_count
        );
        assert_eq!(
            after.state_root_verify_skipped_count,
            before.state_root_verify_skipped_count.saturating_add(1)
        );
    });
}

#[test]
fn state_root_stays_identical_after_refcnt_rebuild() {
    init_stable_state();
    let addr = [0x99u8; 20];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr),
            AccountVal::from_parts(1, [0x0au8; 32], [0u8; 32]),
        );
        state.storage.insert(
            make_storage_key(addr, [0x03u8; 32]),
            U256Val::new([0x44u8; 32]),
        );
    });

    let baseline = with_state_mut(|state| compute_state_root_incremental_with(state, &[addr]));
    with_state_mut(|state| {
        let mut migration = *state.state_root_migration.get();
        migration.phase = evm_db::chain_data::MigrationPhase::BuildRefcnt;
        migration.cursor = 0;
        state.state_root_migration.set(migration);
    });
    let _ = chain::state_root_migration_tick(512);
    let done = chain::state_root_migration_tick(512);
    assert!(done);

    let rebuilt = with_state_mut(|state| compute_state_root_incremental_with(state, &[addr]));
    assert_eq!(baseline, rebuilt);
}

fn hex32(value: [u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in value {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
