# draft: block_commit_safe-9ce347ac

## inferred behavior
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        facts.committed_head >= facts.previous_head
        && (facts.block_gas_limit == 0 || facts.block_gas_used <= facts.block_gas_limit)
        &&
        facts.included_count != 0
        && facts.included_count == facts.staged_count
        && facts.safe_included_count == facts.included_count
    ),
))]
pub fn block_commit_safe(facts: BlockCommitFacts) -> bool

## intended behavior
候補:

```rust
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        facts.committed_head >= facts.previous_head
            && (facts.block_gas_limit == 0 || facts.block_gas_used <= facts.block_gas_limit)
            && facts.included_count != 0
            && facts.included_count == facts.staged_count
            && facts.safe_included_count == facts.included_count
    ),
))]
pub fn block_commit_safe(facts: BlockCommitFacts) -> bool {
    facts.committed_head >= facts.previous_head
        && (facts.block_gas_limit == 0 || facts.block_gas_used <= facts.block_gas_limit)
        && facts.included_count != 0
        && facts.included_count == facts.staged_count
        && facts.safe_included_count == facts.included_count
}
```

事前条件なし。返値 `safe` は実装の boolean 判定と完全一致する、という最小 postcondition。

## anchor
- git_commit: 703d7df1dcdc48c6f15be3733c6da4ec5c6a8dad
- worktree_dirty: true
- source_hash: 9ce347ac9e0c4f7f51bc344f0f3e6e70a86210a46865b67862551dd106555c0f
- semantic_hash: 5faabe5ff1b9ce811054fe93bfb04e7cd61f3fb74bd18a081bfb35d8a3fbe9eb
