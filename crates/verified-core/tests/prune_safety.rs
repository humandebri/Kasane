//! どこで: verified-core pruning safety / 何を: public pure API / なぜ: gate対象の実装ファイルを小さく保つため

use verified_core::prune_safety::{
    PruneTxCleanupInput, block_is_prunable, block_is_retained, prune_boundary_safe,
    prune_tx_cleanup_complete,
};

#[test]
fn prunable_boundary_excludes_retained_range() {
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
