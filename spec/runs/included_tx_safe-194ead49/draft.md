# draft: included_tx_safe-194ead49

## inferred behavior
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        facts.has_tx_index
        && facts.has_receipt
        && facts.receipt_tx_id_matches
        && facts.index_key_matches_tx_id
        && facts.loc_matches_position
        && facts.receipt_matches_position
        && facts.index_matches_position
        && facts.block_number > 0
    ),
))]
pub fn included_tx_safe(facts: IncludedTxFacts) -> bool

## intended behavior
候補:

```rust
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        facts.has_tx_index
            && facts.has_receipt
            && facts.receipt_tx_id_matches
            && facts.index_key_matches_tx_id
            && facts.loc_matches_position
            && facts.receipt_matches_position
            && facts.index_matches_position
            && facts.block_number > 0
    ),
))]
```

戻り値 `safe` は、取引index・receipt存在、ID/key一致、位置整合、block番号正数の全条件が成立する場合のみ `true`。

## anchor
- git_commit: 703d7df1dcdc48c6f15be3733c6da4ec5c6a8dad
- worktree_dirty: true
- source_hash: 194ead498b0ea20a942e6c51fc03289334a32f97bd436054178b5a70c0736378
- semantic_hash: 2102c129c4bbc6e0e0f034436511bf3ab3d5974f19021a40e2413150e3fa1ecf
