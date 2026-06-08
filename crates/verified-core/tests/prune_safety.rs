//! どこで: verified-core pruning safety / 何を: public pure API / なぜ: gate対象の実装ファイルを小さく保つため

use verified_core::prune_safety::{
    block_is_prunable, block_is_retained, prune_boundary_safe, prune_partial_progress_safe_raw,
    prune_query_observation_safe_raw, prune_tx_cleanup_complete, PruneTxCleanupInput,
};
use verified_core::stable_namespace::stable_tx_namespace_disjoint_raw;

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
    assert!(prune_query_observation_safe_raw(12, 10, 1, 1, 0));
    assert!(prune_query_observation_safe_raw(8, 10, 0, 0, 1));
    assert!(!prune_query_observation_safe_raw(8, 10, 0, 0, 0));
    assert!(!prune_query_observation_safe_raw(8, 10, 0, 1, 0));
    assert!(!prune_query_observation_safe_raw(12, 10, 0, 1, 0));
    assert!(!prune_query_observation_safe_raw(12, 10, 1, 0, 0));
    assert!(!prune_query_observation_safe_raw(12, 10, 1, 0, 1));
    assert!(!prune_query_observation_safe_raw(12, 10, 1, 1, 1));
    assert!(!prune_query_observation_safe_raw(12, 10, 0, 2, 2));
    assert!(!prune_query_observation_safe_raw(12, 10, 0, 1, 2));
    assert!(!prune_query_observation_safe_raw(12, 10, 0, 2, 1));
    assert!(!prune_query_observation_safe_raw(12, 10, 2, 1, 0));
}

#[test]
fn prune_partial_progress_keeps_cursor_restartable() {
    assert!(prune_partial_progress_safe_raw(
        1, 5, 1, 6, 7, 10, 6, 5, 1, 1
    ));
    assert!(prune_partial_progress_safe_raw(
        0, 0, 1, 0, 1, 10, 1, 0, 1, 0
    ));
    assert!(prune_partial_progress_safe_raw(
        1, 5, 1, 5, 6, 10, 0, 1, 0, 0
    ));
    assert!(!prune_partial_progress_safe_raw(
        1, 5, 1, 5, 6, 10, 1, 1, 1, 0
    ));
    assert!(!prune_partial_progress_safe_raw(
        1, 6, 1, 5, 6, 10, 1, 1, 1, 0
    ));
    assert!(!prune_partial_progress_safe_raw(
        1, 5, 1, 6, 7, 10, 0, 1, 0, 0
    ));
    assert!(!prune_partial_progress_safe_raw(
        1, 5, 0, 0, 0, 10, 0, 1, 0, 0
    ));
    assert!(!prune_partial_progress_safe_raw(
        1, 5, 1, 6, 6, 10, 1, 1, 1, 0
    ));
    assert!(!prune_partial_progress_safe_raw(
        1, 5, 0, 0, 6, 10, 1, 1, 1, 0
    ));
    assert!(!prune_partial_progress_safe_raw(
        1, 5, 1, 6, 7, 5, 6, 1, 1, 0
    ));
    assert!(!prune_partial_progress_safe_raw(
        1,
        u64::MAX,
        1,
        u64::MAX,
        u64::MAX,
        10,
        0,
        1,
        0,
        0
    ));
    assert!(!prune_partial_progress_safe_raw(
        1, 5, 1, 6, 7, 10, 1, 5, 0, 1
    ));
    assert!(prune_partial_progress_safe_raw(
        1, 5, 1, 5, 6, 10, 0, 11, 0, 1
    ));
    assert!(!prune_partial_progress_safe_raw(
        1, 5, 1, 6, 7, 10, 5, 5, 1, 1
    ));
}

#[test]
fn stable_tx_namespace_requires_strict_memory_order() {
    assert!(stable_tx_namespace_disjoint_raw(8, 9, 10, 11, 16, 37, 58));
    assert!(!stable_tx_namespace_disjoint_raw(8, 9, 10, 10, 16, 37, 58));
    assert!(!stable_tx_namespace_disjoint_raw(8, 9, 10, 11, 16, 16, 58));
    assert!(!stable_tx_namespace_disjoint_raw(8, 9, 10, 11, 16, 37, 37));
}
