//! どこで: verified-core pruning safety / 何を: public pure API / なぜ: gate対象の実装ファイルを小さく保つため

use verified_core::prune_safety::{
    block_is_prunable, block_is_retained, prune_boundary_safe, prune_query_observation_safe_raw,
    prune_tx_cleanup_complete, PruneTxCleanupInput,
};

#[test]
fn prune_prunable_boundary_excludes_retained_range() {
    assert!(block_is_prunable(10, 3, 7));
    assert!(!block_is_prunable(10, 3, 8));
    assert!(!block_is_prunable(10, 0, 1));
    assert!(!block_is_retained(10, 3, 7));
    assert!(block_is_retained(10, 3, 8));
    assert!(block_is_retained(10, 3, 10));
    assert!(!block_is_retained(10, 3, 11));
}

#[test]
fn prune_boundary_is_monotonic_and_retention_safe() {
    assert!(prune_boundary_safe(false, 0, true, 7, 10, 3));
    assert!(prune_boundary_safe(true, 5, true, 7, 10, 3));
    assert!(!prune_boundary_safe(true, 8, true, 7, 10, 3));
    assert!(!prune_boundary_safe(true, 7, true, 8, 10, 3));
    assert!(prune_boundary_safe(true, 7, false, 0, 10, 3));
}

#[test]
fn prune_cleanup_complete_requires_observable_indexes_gone() {
    let clean = PruneTxCleanupInput {
        tx_store: false,
        receipt: false,
        tx_index: false,
        internal_traces: false,
        tx_loc: false,
        seen_tx: false,
    };
    assert!(prune_tx_cleanup_complete(clean));
    assert!(!prune_tx_cleanup_complete(PruneTxCleanupInput {
        receipt: true,
        ..clean
    }));
    assert!(!prune_tx_cleanup_complete(PruneTxCleanupInput {
        tx_loc: true,
        ..clean
    }));
}

#[test]
fn prune_query_observation_rejects_ok_for_pruned_boundary() {
    assert!(prune_query_observation_safe_raw(1, 12, 10, 1, 1, 0));
    assert!(prune_query_observation_safe_raw(1, 8, 10, 0, 0, 1));
    assert!(!prune_query_observation_safe_raw(1, 8, 10, 0, 1, 0));
    assert!(!prune_query_observation_safe_raw(1, 12, 10, 0, 1, 0));
    assert!(!prune_query_observation_safe_raw(1, 12, 10, 1, 0, 1));
    assert!(!prune_query_observation_safe_raw(1, 12, 10, 1, 1, 1));
    assert!(!prune_query_observation_safe_raw(0, 12, 10, 0, 0, 1));
    assert!(!prune_query_observation_safe_raw(0, 12, 10, 0, 2, 2));
    assert!(!prune_query_observation_safe_raw(0, 12, 10, 0, 1, 2));
    assert!(!prune_query_observation_safe_raw(0, 12, 10, 0, 2, 1));
    assert!(!prune_query_observation_safe_raw(2, 12, 10, 1, 1, 0));
}
