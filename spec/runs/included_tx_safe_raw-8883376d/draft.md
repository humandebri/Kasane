# draft: included_tx_safe_raw-8883376d

## inferred behavior
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        has_tx_index
        && has_receipt
        && receipt_tx_id_matches
        && index_key_matches_tx_id
        && loc_matches_position
        && receipt_matches_position
        && index_matches_position
        && block_number > 0
    ),
))]
pub fn included_tx_safe_raw(
    has_tx_index: bool,
    has_receipt: bool,
    receipt_tx_id_matches: bool,
    index_key_matches_tx_id: bool,
    loc_matches_position: bool,
    receipt_matches_position: bool,
    index_matches_position: bool,
    block_number: u64,
) -> bool

## intended behavior
候補:

```rust
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        has_tx_index
        && has_receipt
        && block_number > 0
        && receipt_tx_id_matches
        && index_key_matches_tx_id
        && loc_matches_position
        && receipt_matches_position
        && index_matches_position
    ),
))]
```

実装の評価順に合わせた形。仕様意図は「tx index・receipt・各位置/ID整合性が全て成立し、block number が 0 でない場合のみ safe」。

## anchor
- git_commit: 4aed4c6b20d169ba2d31ba9c585394470dc69edf
- worktree_dirty: true
- source_hash: 8883376d72032a1c2763639c9b66be22fb0d0b0fd41ea54619d9bb82a98c9763
- semantic_hash: 200dc7e13579af9bb069bfa545c6cbd32dd681daffcd07dcf07ca103ae440c2b
