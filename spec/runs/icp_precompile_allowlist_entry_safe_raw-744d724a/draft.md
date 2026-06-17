# draft: icp_precompile_allowlist_entry_safe_raw-744d724a

## inferred behavior
#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        target_len >= 1
        && target_len <= MAX_PRINCIPAL_LEN
        && target_non_anonymous == 1
        && method_len >= 1
        && method_len <= MAX_QUERY_METHOD_LEN
        && method_ascii == 1
    ),
))]
pub fn icp_precompile_allowlist_entry_safe_raw(
    target_len: u64,
    target_non_anonymous: u64,
    method_len: u64,
    method_ascii: u64,
) -> bool

## intended behavior
候補:

```rust
#[cfg_attr(verus_keep_ghost, verus_spec(valid => ensures
    valid == (
        target_len >= 1
        && target_len <= MAX_PRINCIPAL_LEN
        && target_non_anonymous == 1
        && method_len >= 1
        && method_len <= MAX_QUERY_METHOD_LEN
        && method_ascii == 1
    ),
))]
```

要旨:

- 事前条件なし
- 戻り値 `valid` は実装内の安全判定述語と完全一致
- principal は非空、最大長以下、anonymous 不可
- method は非空、最大長以下、ASCII 必須
- `u64` フラグは `1` のみ真として許可

## anchor
- git_commit: f4f6f494d8be02b8375b85c1b1cfb768376f6dff
- worktree_dirty: false
- source_hash: 744d724ab51703c74b38bfa31261e65bbc8b99e0335effaf53e81ab499a5ec9c
- semantic_hash: 0b6b4cea57f29b85514b0aea66d697fd2e3f041a1e3918ce19d3d182c39a70f7
