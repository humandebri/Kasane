# draft: submit_transition_safe-2614d31a

## inferred behavior
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == match facts.decision

## intended behavior
候補:

```rust
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == match facts.decision {
        NonceDecision::Accept =>
            facts.pending_slot_points_to_new
                && facts.new_current_written
                && facts.queued_loc_written,
        NonceDecision::Replace =>
            facts.pending_slot_points_to_new
                && facts.new_current_written
                && facts.queued_loc_written
                && facts.replacement_old_removed,
        NonceDecision::TooLow | NonceDecision::Gap | NonceDecision::Conflict => false,
    }
))]
```

`requires`なし。戻り値`safe`が、`decision`ごとの必要事実の論理積と完全一致する仕様。

## anchor
- git_commit: 703d7df1dcdc48c6f15be3733c6da4ec5c6a8dad
- worktree_dirty: true
- source_hash: 2614d31a4a4907697ec3e02f19047ef7fb96231eec06496748eb14bbea2301d7
- semantic_hash: 9d7662472f7122ee0694fbdaaec25fea77edb91adbbba0e8e01228a3050eff4a
