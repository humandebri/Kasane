//! どこで: verified-core prune integration tests / 何を: 公開prune API / なぜ: 実装ファイルを小さく保つため

use verified_core::prune::{
    advance_after_pruned_block, clamp_max_ops, high_water, low_water, need_prune,
    normalize_next_prune_block, prune_before_block, prune_ops_needed_for_tx, ratio_bytes,
    recover_pruned_before, remaining_blocks, retain_count, PruneCursor, PruneTxPresence,
    RetainCountInput,
};

#[test]
fn watermarks_preserve_order_under_headroom() {
    let high = high_water(1_000_000, 2_000);
    let low = low_water(1_000_000, 2_000);
    assert!(high > low);
    assert!(high <= 1_000_000);
}

#[test]
fn ratio_rounds_down_and_clamp_raises_minimum() {
    assert_eq!(ratio_bytes(1000, 3333), 333);
    assert_eq!(ratio_bytes(u64::MAX, u32::MAX), u64::MAX);
    assert_eq!(clamp_max_ops(0, 1), 1);
    assert_eq!(clamp_max_ops(10, 1), 10);
}

#[test]
fn need_prune_tracks_time_and_capacity_triggers() {
    assert!(need_prune(1, Some(0), 200_000, 0, 0, 0));
    assert!(!need_prune(1, None, 200_000, 0, 0, 0));
    assert!(need_prune(0, None, 0, 100, 90, 80));
    assert!(!need_prune(0, None, 0, 100, 80, 80));
}

#[test]
fn retain_count_uses_capacity_and_retention_policy() {
    let base = RetainCountInput {
        head_block: 100,
        target_bytes: 0,
        estimated_kept_bytes: 0,
        high_water_bytes: 0,
        hard_emergency_bytes: 0,
        retain_blocks: 10,
        retain_days: 0,
        cutoff_block: None,
    };
    assert_eq!(retain_count(base), 10);
    assert_eq!(
        retain_count(RetainCountInput {
            retain_days: 1,
            cutoff_block: Some(95),
            ..base
        }),
        6
    );
    assert_eq!(
        retain_count(RetainCountInput {
            target_bytes: 100,
            estimated_kept_bytes: 91,
            high_water_bytes: 80,
            hard_emergency_bytes: 90,
            ..base
        }),
        1
    );
}

#[test]
fn prune_cursor_transitions_are_monotonic() {
    assert_eq!(prune_before_block(10, 3), Some(7));
    assert_eq!(prune_before_block(3, 3), None);
    assert_eq!(
        normalize_next_prune_block(PruneCursor {
            next_prune_block: 4,
            pruned_before_block: Some(5),
        }),
        6
    );
    assert_eq!(
        normalize_next_prune_block(PruneCursor {
            next_prune_block: u64::MAX - 1,
            pruned_before_block: Some(u64::MAX),
        }),
        u64::MAX
    );
    assert_eq!(advance_after_pruned_block(9).next_prune_block, 10);
    assert_eq!(recover_pruned_before(Some(8), 7), Some(8));
    assert_eq!(recover_pruned_before(Some(8), 9), Some(9));
    assert_eq!(remaining_blocks(5, 7), 3);
    assert_eq!(remaining_blocks(8, 7), 0);
    assert_eq!(remaining_blocks(0, u64::MAX), u64::MAX);
}

#[test]
fn prune_tx_ops_count_present_indexes() {
    assert_eq!(
        prune_ops_needed_for_tx(PruneTxPresence {
            pending_fee_index: false,
            principal_pending_count: false,
            eth_tx_hash_index: false,
            tx_store: false,
            receipt: false,
            tx_index: false,
            internal_traces: false,
            tx_loc: false,
            seen_tx: false,
        }),
        0
    );
    assert_eq!(
        prune_ops_needed_for_tx(PruneTxPresence {
            pending_fee_index: true,
            principal_pending_count: true,
            eth_tx_hash_index: false,
            tx_store: true,
            receipt: true,
            tx_index: true,
            internal_traces: false,
            tx_loc: true,
            seen_tx: true,
        }),
        7
    );
    assert_eq!(
        prune_ops_needed_for_tx(PruneTxPresence {
            pending_fee_index: true,
            principal_pending_count: true,
            eth_tx_hash_index: true,
            tx_store: true,
            receipt: true,
            tx_index: true,
            internal_traces: true,
            tx_loc: true,
            seen_tx: true,
        }),
        9
    );
}
