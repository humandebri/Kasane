# draft: staged_tx_is_current_pending_raw-8f305091

## inferred behavior
pub fn staged_tx_is_current_pending_raw(
    ready_points_to_tx: u64,
    pending_meta_points_to_tx: u64,
    current_pending_points_to_tx: u64,
    tx_payload_present: u64,
    tx_not_marked_dropped: u64,
) -> bool

## intended behavior
仕様案:

```rust
requires true
ensures result == (
    ready_points_to_tx == 1
        && pending_meta_points_to_tx == 1
        && current_pending_points_to_tx == 1
        && tx_payload_present == 1
        && tx_not_marked_dropped == 1
)
```

受入基準:
- 5入力がすべて `1` の場合のみ `true`
- いずれか1つでも `1` 以外なら `false`
- `0` と `2以上` は同等に不成立扱い

## anchor
- git_commit: 45c236f431ea13639e1ce09e51a6e84f7b627d28
- worktree_dirty: true
- source_hash: 8f3050911d0e6e3e05e12e96739a545c70d0c0c8a35e7098ea55c3984a12602c
- semantic_hash: 01e3253152d39ab3884ccbcaae345bfb0276e2f35d25346253301e0ba55ddc6d
