# draft: upgrade_core_observation_preserved_raw-191130e4

## inferred behavior
pub fn upgrade_core_observation_preserved_raw(head_same: u64, pruned_boundary_same: u64, pending_same: u64, receipt_same: u64, tx_index_same: u64, tx_loc_same: u64) -> bool

## intended behavior
候補:

```rust
requires true
ensures result <==>
    head_same == 1
    && pruned_boundary_same == 1
    && pending_same == 1
    && receipt_same == 1
    && tx_index_same == 1
    && tx_loc_same == 1
```

受入基準:

- 6個の観測値がすべて `1` の場合のみ `true`。
- `0` や `2` 以上を含む任意の値は `false`。
- `u64` 入力に追加制約なし。

## anchor
- git_commit: 2d344b0fd7f9384a0aa23cb9683b0a9c62aa9ef3
- worktree_dirty: true
- source_hash: 191130e4b8b76d4409ea2457fd792448803236c01a7acc1f282fcf3cb7df6b93
- semantic_hash: 7dea973b8bd70f123edd509a26aea84e43cc7c3f136c1bebdfdb5883999d9489
