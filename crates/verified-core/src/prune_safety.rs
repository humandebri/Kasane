//! どこで: pruning安全性 / 何を: 純粋境界モデルの公開口 / なぜ: pruning実装証拠を小さな証明対象へ分割するため

pub mod block_prunable;
pub mod block_retained;
pub mod boundary;
pub mod cleanup;

pub use block_prunable::block_is_prunable;
pub use block_retained::block_is_retained;
pub use boundary::prune_boundary_safe;
pub use cleanup::{PruneTxCleanupInput, prune_tx_cleanup_complete};

#[cfg(test)]
mod tests {
    use super::{
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
        assert!(prune_boundary_safe(None, Some(7), 10, 3));
        assert!(prune_boundary_safe(Some(5), Some(7), 10, 3));
        assert!(!prune_boundary_safe(Some(8), Some(7), 10, 3));
        assert!(!prune_boundary_safe(Some(7), Some(8), 10, 3));
        assert!(prune_boundary_safe(Some(7), None, 10, 3));
    }

    #[test]
    fn cleanup_complete_requires_observable_indexes_gone() {
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
}
