# draft: icp_query_update_kind_rejected_raw-4de9db5f

## inferred behavior
#[cfg_attr(verus_keep_ghost, verus_spec(rejected => ensures
    rejected == (kind == ICP_QUERY_KIND_UPDATE_RESERVED),
))]
pub fn icp_query_update_kind_rejected_raw(kind: u64) -> bool

## intended behavior
候補:

```rust
#[cfg_attr(verus_keep_ghost, verus_spec(
    ensures
        result == (kind == ICP_QUERY_KIND_UPDATE_RESERVED),
))]
pub fn icp_query_update_kind_rejected_raw(kind: u64) -> bool {
    kind == ICP_QUERY_KIND_UPDATE_RESERVED
}
```

要点:
- `requires` なし。全 `u64` 入力で定義済み。
- `result` は Verus/specgen の返値名に合わせる。
- `rejected` 名を使うなら、specgen標準の postcondition ではなく手書き属性向け。

## anchor
- git_commit: f4f6f494d8be02b8375b85c1b1cfb768376f6dff
- worktree_dirty: false
- source_hash: 4de9db5f9a80647d86118783b6d903c928b9f3b1395fb025e0cecb6f93c10c17
- semantic_hash: a8ddb97d337ebf1eee99dbf008dafc4e3f2817925518eef25009600d997b6d5d
