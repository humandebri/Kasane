# draft: prune_query_observation_safe_raw-cb39bc8e

## inferred behavior
pub fn prune_query_observation_safe_raw(
    block_number: u64,
    pruned_through: u64,
    retained: u64,
    returned_ok: u64,
    returned_pruned: u64,
) -> bool

## intended behavior
候補:

```rust
/// Query observation is safe iff exactly one normalized outcome is possible:
/// - pruned block: no retained/ok observation, one pruned result
/// - retained block: no pruned result, retained count matches ok results
///
/// All observation counters are bounded to at most one.
pub fn prune_query_observation_safe_raw(
    block_number: u64,
    pruned_through: u64,
    retained: u64,
    returned_ok: u64,
    returned_pruned: u64,
) -> bool {
    retained <= 1
        && returned_ok <= 1
        && returned_pruned <= 1
        && if block_number <= pruned_through {
            retained == 0 && returned_ok == 0 && returned_pruned == 1
        } else {
            returned_pruned == 0 && retained == returned_ok
        }
}
```

要点:
- `block_number <= pruned_through` → query は pruning 済みとして観測される
- `pruned_through < block_number` → query は pruning されず、保持状態と成功返却が一致する
- 各観測値は boolean count として `0..=1` に制限する

## anchor
- git_commit: e5653c859a82d960de279312d0d9447f85c3ab1f
- worktree_dirty: false
- source_hash: f995e92b50e4742b6dad30a894153c55aa064e306118b40cfcf7e1bbd8f2c608
- semantic_hash: 445d7982acc1963af76d4baf9875f22f0cc5103df8aceed986503da7aa4b5d62
