//! どこで: pruning判定のテスト / 何を: time/cap/hard_emergency / なぜ: 仕様の固定化

use std::sync::{Mutex, OnceLock};

use evm_core::chain;
use evm_db::chain_data::{BlockData, PrunePolicy, TxId};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
use evm_db::Storable;

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn time_trigger_works_without_target_bytes() {
    let _guard = test_lock().lock().unwrap();
    init_stable_state();
    with_state_mut(|state| {
        let mut head = *state.head.get();
        head.number = 1;
        head.timestamp = 200_000;
        state.head.set(head);
        let mut config = *state.prune_config.get();
        config.target_bytes = 0;
        config.retain_days = 1;
        config.set_oldest(0, 0);
        state.prune_config.set(config);
    });
    let status = chain::get_prune_status();
    assert!(status.need_prune);
}

#[test]
fn capacity_trigger_sets_need_prune() {
    let _guard = test_lock().lock().unwrap();
    init_stable_state();
    with_state_mut(|state| {
        let mut head = *state.head.get();
        head.number = 1;
        head.timestamp = 1_000;
        state.head.set(head);
        let block = make_block(1);
        insert_block(state, 1, &block);
        let mut config = *state.prune_config.get();
        let policy = PrunePolicy {
            target_bytes: 100,
            retain_days: 0,
            retain_blocks: 0,
            headroom_ratio_bps: 2000,
            hard_emergency_ratio_bps: 9500,
            max_ops_per_tick: 1_000,
        };
        config.set_policy(policy);
        config.set_oldest(0, 0);
        state.prune_config.set(config);
    });
    let status = chain::get_prune_status();
    assert!(status.need_prune);
}

#[test]
fn hard_emergency_prunes_down_to_one_block() {
    let _guard = test_lock().lock().unwrap();
    init_stable_state();
    with_state_mut(|state| {
        for number in 0..=3 {
            let block = make_block(number);
            insert_block(state, number, &block);
        }
        let mut head = *state.head.get();
        head.number = 3;
        head.timestamp = 10_000;
        state.head.set(head);
        let mut config = *state.prune_config.get();
        let policy = PrunePolicy {
            target_bytes: 100,
            retain_days: 0,
            retain_blocks: 0,
            headroom_ratio_bps: 2000,
            hard_emergency_ratio_bps: 9000,
            max_ops_per_tick: 10_000,
        };
        config.set_policy(policy);
        config.pruning_enabled = true;
        config.set_oldest(0, 0);
        state.prune_config.set(config);
    });
    let result = chain::prune_tick().expect("prune_tick should succeed");
    assert_eq!(result.pruned_before_block, Some(2));
}

#[test]
fn estimated_kept_bytes_drops_after_prune_and_can_clear_cap_trigger() {
    let _guard = test_lock().lock().unwrap();
    init_stable_state();
    with_state_mut(|state| {
        for number in 0..=3 {
            let block = make_block(number);
            insert_block(state, number, &block);
        }
        let mut head = *state.head.get();
        head.number = 3;
        head.timestamp = 10_000;
        state.head.set(head);
        let mut config = *state.prune_config.get();
        let policy = PrunePolicy {
            target_bytes: 1,
            retain_days: 0,
            retain_blocks: 0,
            headroom_ratio_bps: 2000,
            hard_emergency_ratio_bps: 9000,
            max_ops_per_tick: 10_000,
        };
        config.set_policy(policy);
        config.pruning_enabled = true;
        config.set_oldest(0, 0);
        state.prune_config.set(config);
    });
    let before = chain::get_prune_status();
    let before_blob_used = with_state(|state| state.blob_store.usage_stats().used_class_bytes);
    assert!(before.need_prune);
    let result = chain::prune_tick().expect("prune_tick should succeed");
    assert!(result.did_work, "prune_tick should do work under cap trigger");
    let after = chain::get_prune_status();
    let after_blob_used = with_state(|state| state.blob_store.usage_stats().used_class_bytes);
    assert!(
        after_blob_used < before_blob_used,
        "blob used bytes should decrease after prune"
    );
    with_state_mut(|state| {
        let mut config = *state.prune_config.get();
        let policy = PrunePolicy {
            target_bytes: after.estimated_kept_bytes.saturating_add(1_000_000),
            retain_days: 0,
            retain_blocks: 0,
            headroom_ratio_bps: 2000,
            hard_emergency_ratio_bps: 9000,
            max_ops_per_tick: 10_000,
        };
        config.set_policy(policy);
        state.prune_config.set(config);
    });
    let relaxed = chain::get_prune_status();
    assert!(!relaxed.need_prune);
}

fn make_block(number: u64) -> BlockData {
    let parent_hash = [0u8; 32];
    let number_u8 = u8::try_from(number).unwrap_or(0);
    let block_hash = [number_u8; 32];
    let tx_list_hash = [number_u8; 32];
    let state_root = [0u8; 32];
    BlockData::new(
        number,
        parent_hash,
        block_hash,
        number,
        1_000_000_000,
        3_000_000,
        0,
        Vec::<TxId>::new(),
        tx_list_hash,
        state_root,
    )
}

fn insert_block(state: &mut evm_db::stable_state::StableState, number: u64, block: &BlockData) {
    let bytes = block.to_bytes().into_owned();
    let ptr = state.blob_store.store_bytes(&bytes).expect("store block");
    state.blocks.insert(number, ptr);
}
