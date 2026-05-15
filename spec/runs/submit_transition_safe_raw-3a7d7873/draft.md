# draft: submit_transition_safe_raw-3a7d7873

## inferred behavior
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        (decision_code == SUBMIT_DECISION_ACCEPT
            && pending_slot_points_to_new
            && new_current_written
            && queued_loc_written
            && !replacement_old_removed)
        || (decision_code == SUBMIT_DECISION_REPLACE
            && pending_slot_points_to_new
            && new_current_written
            && queued_loc_written
            && replacement_old_removed)
    ),
))]
pub fn submit_transition_safe_raw(
    decision_code: u8,
    pending_slot_points_to_new: bool,
    new_current_written: bool,
    queued_loc_written: bool,
    replacement_old_removed: bool,
) -> bool

## intended behavior
候補:

```rust
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        pending_slot_points_to_new
        && new_current_written
        && queued_loc_written
        && (
            (decision_code == SUBMIT_DECISION_ACCEPT && !replacement_old_removed)
            || (decision_code == SUBMIT_DECISION_REPLACE && replacement_old_removed)
        )
    ),
))]
```

事前条件なし。  
意味: `ACCEPT` は旧削除なし、`REPLACE` は旧削除ありの場合だけ安全。共通条件は pending/current/queue の3書込成立。

## anchor
- git_commit: 4aed4c6b20d169ba2d31ba9c585394470dc69edf
- worktree_dirty: true
- source_hash: 3a7d787348db3ccbe4c6f3378ac49f0d6c3a350b3f96ad2c9c9949fc6640a9e6
- semantic_hash: 5a42d752fbfb9dbd515f43b51b2822024ca162427dfa173e19451af00a27605d
