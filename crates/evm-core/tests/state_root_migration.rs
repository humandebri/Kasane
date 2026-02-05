//! どこで: state_root migrationテスト / 何を: phase進行と再実行性を検証 / なぜ: 中断後再開の安全性を担保するため

use evm_core::chain;
use evm_db::chain_data::MigrationPhase;
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
use evm_db::types::keys::{make_account_key, make_storage_key};
use evm_db::types::values::{AccountVal, U256Val};

#[test]
fn migration_tick_reaches_done_idempotently() {
    init_stable_state();
    with_state_mut(|state| {
        let mut m = *state.state_root_migration.get();
        m.phase = MigrationPhase::Init;
        m.cursor = 0;
        state.state_root_migration.set(m);
    });

    let mut done = false;
    for _ in 0..10 {
        done = chain::state_root_migration_tick(128);
        if done {
            break;
        }
    }
    assert!(done);
    let phase = with_state(|state| state.state_root_migration.get().phase);
    assert_eq!(phase, MigrationPhase::Done);

    // Done後の再実行は副作用なくtrueを返す。
    assert!(chain::state_root_migration_tick(128));
    let phase_again = with_state(|state| state.state_root_migration.get().phase);
    assert_eq!(phase_again, MigrationPhase::Done);
}

#[test]
fn migration_build_refcnt_populates_node_db() {
    init_stable_state();
    let addr = [0xabu8; 20];
    with_state_mut(|state| {
        state.accounts.insert(
            make_account_key(addr),
            AccountVal::from_parts(1, [0u8; 32], [0u8; 32]),
        );
        state.storage.insert(
            make_storage_key(addr, [0x01u8; 32]),
            U256Val::new([0x11u8; 32]),
        );
        let mut m = *state.state_root_migration.get();
        m.phase = MigrationPhase::BuildRefcnt;
        state.state_root_migration.set(m);
    });

    let done = chain::state_root_migration_tick(128);
    assert!(!done);
    let (node_entries, leaf_entries, dangling_leaf_refs) = with_state(|state| {
        let node_entries = state.state_root_node_db.len();
        let leaf_entries = state.state_root_account_leaf_hash.len();
        let mut dangling = 0u64;
        for entry in state.state_root_account_leaf_hash.iter() {
            let hash = entry.value();
            let ok = state
                .state_root_node_db
                .get(&hash)
                .map(|r| r.refcnt > 0)
                .unwrap_or(false);
            if !ok {
                dangling = dangling.saturating_add(1);
            }
        }
        (node_entries, leaf_entries, dangling)
    });
    assert!(node_entries > 0);
    assert!(leaf_entries > 0);
    assert_eq!(dangling_leaf_refs, 0);

    // Verify -> Done
    let done2 = chain::state_root_migration_tick(128);
    assert!(done2);
    let phase = with_state(|state| state.state_root_migration.get().phase);
    assert_eq!(phase, MigrationPhase::Done);
}

#[test]
fn migration_build_trie_progresses_with_small_steps() {
    init_stable_state();
    for i in 0..4u8 {
        let addr = [i + 1; 20];
        with_state_mut(|state| {
            state.accounts.insert(
                make_account_key(addr),
                AccountVal::from_parts(1, [0u8; 32], [0u8; 32]),
            );
            state.storage.insert(
                make_storage_key(addr, [0x01u8; 32]),
                U256Val::new([i + 1; 32]),
            );
        });
    }
    with_state_mut(|state| {
        let mut m = *state.state_root_migration.get();
        m.phase = MigrationPhase::BuildTrie;
        m.cursor = 0;
        state.state_root_migration.set(m);
    });

    let mut progressed = false;
    for _ in 0..32 {
        let _ = chain::state_root_migration_tick(1);
        let state = with_state(|s| *s.state_root_migration.get());
        if state.phase != MigrationPhase::BuildTrie {
            progressed = true;
            break;
        }
        if state.cursor > 0 {
            progressed = true;
        }
    }
    assert!(progressed, "migration cursor/phase must progress with max_steps=1");
}
