# draft: submit_transition_safe-5e9926e3

## inferred behavior
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == match facts.decision

## intended behavior
候補:

```rust
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe ==
        facts.pending_slot_points_to_new
        && facts.new_current_written
        && facts.queued_loc_written
        && match facts.decision {
            NonceDecision::Accept => !facts.replacement_old_removed,
            NonceDecision::Replace => facts.replacement_old_removed,
            NonceDecision::TooLow | NonceDecision::Gap | NonceDecision::Conflict => false,
        },
))]
```

## anchor
- git_commit: 0e1cc96bcf8c44bcd8209096be775abafd9ae137
- worktree_dirty: true
- source_hash: 5e9926e385ee27de688cf107e99eccd13e8f2757b8a744c830a289080e6ed65c
- semantic_hash: 7100c28757c0fd9171871615827268bf49a87efed9ba4d7bd532f84067ec568d
