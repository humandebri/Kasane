# draft: icp_query_gas_observation_safe_raw-ae357da2

## inferred behavior
#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        observed_address_code == ICP_QUERY_PRECOMPILE_ADDRESS_CODE
        && returned_success <= 1
        && (input_len <= MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS
            && reply_len <= MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS
            ==> charged_gas >= ICP_QUERY_BASE_GAS
                + input_len * ICP_QUERY_INPUT_BYTE_GAS
                + reply_len * ICP_QUERY_REPLY_BYTE_GAS)
        && (returned_success == 1 ==> gas_limit >= charged_gas)
        && (returned_success == 0 ==> gas_limit < charged_gas)
    ),
))]
pub fn icp_query_gas_observation_safe_raw(
    observed_address_code: u64,
    input_len: u64,
    reply_len: u64,
    charged_gas: u64,
    gas_limit: u64,
    returned_success: u64,
) -> bool

## intended behavior
仕様候補:

```rust
#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        observed_address_code == ICP_QUERY_PRECOMPILE_ADDRESS_CODE
        && returned_success <= 1
        && (
            input_len <= MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS
            && reply_len <= MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS
            ==> charged_gas >= ICP_QUERY_BASE_GAS
                + input_len * ICP_QUERY_INPUT_BYTE_GAS
                + reply_len * ICP_QUERY_REPLY_BYTE_GAS
        )
        && (returned_success == 1 ==> gas_limit >= charged_gas)
        && (returned_success == 0 ==> gas_limit < charged_gas)
    ),
))]
```

要点:
- address code一致
- `returned_success` は `0 | 1`
- exact gas対象範囲内なら最低課金額を満たす
- successなら `gas_limit >= charged_gas`
- failureなら `gas_limit < charged_gas`

注意: `u64` 乗算・加算のoverflowをVerus側で厳密化するなら、別途 `requires` か上限条件を追加する。

## anchor
- git_commit: 99e52aaefad61f61c45b8900e6011bd9194ff502
- worktree_dirty: false
- source_hash: ae357da24440cc96cd803023854a33e2ef814e9b53db9b2fcbd8eb036802a2ae
- semantic_hash: 18e4342ead1acdbd2d672efcd2aab4913b2b8ee7904adc8dab1a8d60765c39bd
