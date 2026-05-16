//! どこで: pruning安全性 / 何を: 純粋境界モデルの公開口 / なぜ: pruning実装証拠を小さな証明対象へ分割するため

pub mod block_prunable;
pub mod block_retained;
pub mod boundary;
pub mod cleanup;

pub use block_prunable::block_is_prunable;
pub use block_retained::block_is_retained;
pub use boundary::prune_boundary_safe;
pub use cleanup::{PruneTxCleanupInput, prune_tx_cleanup_complete};
