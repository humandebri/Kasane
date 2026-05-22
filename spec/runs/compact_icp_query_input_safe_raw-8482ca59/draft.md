# draft: compact_icp_query_input_safe_raw-8482ca59

## inferred behavior
#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        version == COMPACT_FORMAT_VERSION
        && kind == ICP_QUERY_KIND_QUERY
        && target_len >= 1
        && target_len <= MAX_PRINCIPAL_LEN
        && target_present == 1
        && method_len >= 1
        && method_len <= MAX_QUERY_METHOD_LEN
        && method_present == 1
        && method_utf8 == 1
        && arg_present == 1
        && consumed_exact == 1
    ),
))]
pub fn compact_icp_query_input_safe_raw(
    version: u64,
    kind: u64,
    target_len: u64,
    target_present: u64,
    method_len: u64,
    method_present: u64,
    method_utf8: u64,
    arg_present: u64,
    consumed_exact: u64,
) -> bool

## intended behavior
仕様候補:

```rust
/// Compact ICP query input is valid iff all structural flags and bounds match
/// the supported query envelope format.
#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        version == COMPACT_FORMAT_VERSION
        && kind == ICP_QUERY_KIND_QUERY
        && target_len >= 1
        && target_len <= MAX_PRINCIPAL_LEN
        && target_present == 1
        && method_len >= 1
        && method_len <= MAX_QUERY_METHOD_LEN
        && method_present == 1
        && method_utf8 == 1
        && arg_present == 1
        && consumed_exact == 1
    ),
))]
```

要点: 戻り値 `valid` は関数本体の判定式と完全一致。入力の妥当性は形式バージョン、query種別、target/methodの存在と長さ、method UTF-8、arg存在、完全消費で定義する。

## anchor
- git_commit: f4f6f494d8be02b8375b85c1b1cfb768376f6dff
- worktree_dirty: false
- source_hash: 8482ca59317370ee896eb92d00727b456dd012c5508548d625deb1a86d97129c
- semantic_hash: f883212fd840dab907aa4572153c87a087ed775c8c2a06e79d079e4382a1ec4c
