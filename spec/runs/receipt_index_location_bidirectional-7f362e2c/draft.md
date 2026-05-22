# draft: receipt_index_location_bidirectional-7f362e2c

## inferred behavior
pub fn receipt_index_location_bidirectional(input: ReceiptIndexObservation) -> bool

## intended behavior
仕様候補:

```rust
ensures
    result
        == ((!input.tx_index_present
            && !input.receipt_present
            && !input.included_loc_present)
            || (input.tx_index_present
                && input.receipt_present
                && input.included_loc_present
                && input.index_matches_loc
                && input.receipt_matches_loc
                && input.loc_points_to_block_tx))
```

要旨:

`receipt_index_location_bidirectional` は、receipt index / receipt / included location の観測が「全欠損」または「全存在かつ相互一致」の場合だけ `true` を返す。部分的な存在、location 不一致、location が block transaction を指さない場合は `false`。

## anchor
- git_commit: 45c236f431ea13639e1ce09e51a6e84f7b627d28
- worktree_dirty: true
- source_hash: 7f362e2c26ea1e9b56eda044aaac97c8e7ea546c2e33dee844350504411674bb
- semantic_hash: d553b2dd715ff607be716b4f3e729a79528c7d9b7f58393e9d3285a37e135dc0
