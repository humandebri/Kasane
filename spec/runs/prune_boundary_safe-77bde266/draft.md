# draft: prune_boundary_safe-77bde266

## inferred behavior
pub fn prune_boundary_safe(previous_present: bool, previous: u64, next_present: bool, next_boundary: u64, head: u64, retain: u64) -> bool

## intended behavior
仕様候補:

```rust
// preconditions
true

// postcondition
result == (
    !next_present
    || (
        retain > 0
        && head > retain
        && (next_boundary as int) <= (head as int) - (retain as int)
        && (!previous_present || previous <= next_boundary)
    )
)
```

受入基準:

- `next_present == false` なら常に `true`
- `next_present == true` かつ `retain == 0` なら `false`
- `next_present == true` かつ `head <= retain` なら `false`
- `next_boundary` が保持範囲内、つまり `next_boundary > head - retain` なら `false`
- `previous_present == true` の場合、`previous <= next_boundary` のときだけ `true`
- `previous_present == false` の場合、前方境界の順序条件は不要

代表シナリオ:

```text
no_next_boundary_is_safe
next_with_zero_retain_is_not_safe
head_not_beyond_retain_is_not_safe
next_boundary_inside_retention_window_is_not_safe
previous_after_next_boundary_is_not_safe
eligible_boundary_without_previous_is_safe
eligible_boundary_with_ordered_previous_is_safe
```

## anchor
- git_commit: 18e8ee8e7baf1cee11784c88c37c0ffc190b418a
- worktree_dirty: false
- source_hash: 77bde2669656453e5bf1ca19273a79287ec5f39c84ba1b3fbae22f182129a75a
- semantic_hash: 5c9a4a620ffa373b8f9e9788b515284f4d6537295c9dc43d1f04232aada60702
