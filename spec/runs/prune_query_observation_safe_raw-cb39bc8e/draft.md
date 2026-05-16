# draft: prune_query_observation_safe_raw-cb39bc8e

## inferred behavior
pub fn prune_query_observation_safe_raw(
    boundary_present: u64,
    block_number: u64,
    pruned_before: u64,
    retained: u64,
    returned_ok: u64,
    returned_pruned: u64,
) -> bool

## intended behavior
仕様案:

```text
prune_query_observation_safe_raw は、query 観測結果が prune 境界と保持状態に矛盾しない場合に true を返す。

前提:
- boundary_present, retained, returned_ok, returned_pruned は 0/1 のフラグ値。

安全条件:
- returned_ok は retained が存在する場合のみ許可する。
- boundary_present があり、block_number <= pruned_before の場合、returned_ok は禁止する。
- retained が存在する場合、returned_pruned は禁止する。
- returned_ok と returned_pruned は同時に成立しない。
- returned_pruned は boundary_present があり、かつ block_number <= pruned_before の場合のみ許可する。

補足:
- pruned 対象 block に必ず returned_pruned を要求する仕様ではない。
- retained がなくても、returned_ok/returned_pruned が両方 0 なら許可される。
```

## anchor
- git_commit: 1946d97dae8b1b5f03e38b24849fe6f09b96c178
- worktree_dirty: true
- source_hash: 0cec26b2b449d548ec517d910fa5ec0a9839e7dd1d2cf9b64284637633f6762e
- semantic_hash: 74d83fba47ec171c218ec8f4d2199adc4e63e2b175e3743f496b73fcdc7e37d3
