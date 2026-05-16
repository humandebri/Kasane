# draft: block_is_retained-9d9115e5

## inferred behavior
pub fn block_is_retained(head: u64, retain: u64, block: u64) -> bool

## intended behavior
```rust
// Preconditions:
// - none; all u64 inputs are valid.
//
// Postconditions:
// - result is false when block is ahead of head.
// - result is true for any block <= head when retain == 0.
// - result is true for any block <= head when head <= retain.
// - otherwise, result is true exactly when block > head - retain.
ensures
    result == (
        block <= head &&
        (
            retain == 0 ||
            head <= retain ||
            block > head - retain
        )
    )
```

境界条件:
- `block == head + 1` → `false`
- `retain == 0 && block <= head` → `true`
- `head == retain && block <= head` → `true`
- `head > retain && block == head - retain` → `false`
- `head > retain && block == head - retain + 1` → `true`

## anchor
- git_commit: e7c4bd6deef99685a57a519fa4643f44b1ee134b
- worktree_dirty: false
- source_hash: 9d9115e5f68e8345ecdf9b116dc5efd5cb5860d1fcf943769e0eb6de9e55bbcd
- semantic_hash: e4af40e6fe1aa259aaabaaa386b6780157b0e07a3d8ec08a9c1babe2055bad9f
