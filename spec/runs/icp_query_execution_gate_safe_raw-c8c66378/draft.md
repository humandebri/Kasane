# draft: icp_query_execution_gate_safe_raw-c8c66378

## inferred behavior
#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        calls_before == 0
        && mode_allows_external == 1
        && value_is_zero == 1
        && parsed_input == 1
    ),
))]
pub fn icp_query_execution_gate_safe_raw(
    calls_before: u64,
    mode_allows_external: u64,
    value_is_zero: u64,
    parsed_input: u64,
) -> bool

## intended behavior
候補:

```rust
#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        calls_before == 0
        && mode_allows_external == 1
        && value_is_zero == 1
        && parsed_input == 1
    ),
))]
```

意味: 戻り値 `valid` は、外部呼び出し前で、外部実行許可・ゼロ値・入力解析済みの全フラグが成立する場合だけ `true`。範囲制約は不要。`1` 以外の値はすべて不成立として扱う。

## anchor
- git_commit: 99e52aaefad61f61c45b8900e6011bd9194ff502
- worktree_dirty: false
- source_hash: c8c66378a334edaa890f62987b18dc9d89116acd795898f195e2b891ee1710f3
- semantic_hash: 8204207021ba904b1a7feb073aba9a6903f7674a6a197baa080817f53f3cca1a
