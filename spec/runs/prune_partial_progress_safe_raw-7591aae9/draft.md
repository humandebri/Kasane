# draft: prune_partial_progress_safe_raw-7591aae9

## inferred behavior
pub fn prune_partial_progress_safe_raw(
    previous_present: u64,
    previous_boundary: u64,
    next_present: u64,
    next_boundary: u64,
    next_cursor: u64,
    max_ops: u64,
    ops_used: u64,
    next_ops_needed: u64,
    did_work: u64,
    stopped_for_budget: u64,
) -> bool

## intended behavior
候補:

```rust
requires true

ensures result == (
    previous_present <= 1
    && next_present <= 1
    && did_work <= 1
    && stopped_for_budget <= 1
    && ops_used <= max_ops
    && (did_work == 0 || next_present == 1)
    && (previous_present == 0 || next_present == 1)
    && (did_work == 0 || previous_present == 0 || previous_boundary < next_boundary)
    && (previous_present == 0
        || next_present == 0
        || previous_boundary == next_boundary
        || did_work == 1)
    && (previous_present == 0 || next_present == 0 || previous_boundary <= next_boundary)
    && (next_present == 0 || next_boundary < next_cursor)
    && (stopped_for_budget == 0 || next_present == 1)
    && (stopped_for_budget == 0
        || max_ops < next_ops_needed
        || (next_ops_needed <= max_ops && max_ops - next_ops_needed < ops_used))
)
```

要点:
- `did_work == 1` なら次状態が存在し、既存境界がある場合は境界が前進する。
- 既存境界がある場合、次状態は消えない。
- 未作業で前後状態がある場合、境界は同一。
- 予算停止時は次状態が存在し、次操作が予算超過または残予算不足。

## anchor
- git_commit: e352489ad3eea17d8af4d095ff0a9b33803abc51
- worktree_dirty: true
- source_hash: c6ab22f8e8e9f187fcdd99485ae56548b5945a0a6daaf6a8f0c86fb907b4951c
- semantic_hash: ced03b5267e4b198fbe6bc0ac83caaed2b77abbb4c68bace0d109aff3f435aa0
