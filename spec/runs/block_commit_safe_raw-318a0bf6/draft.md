# draft: block_commit_safe_raw-318a0bf6

## inferred behavior
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        committed_head >= previous_head
        && (block_gas_limit == 0 || block_gas_used <= block_gas_limit)
        && included_count != 0
        && included_count == staged_count
        && safe_included_count == included_count
    ),
))]
pub fn block_commit_safe_raw(
    previous_head: u64,
    committed_head: u64,
    included_count: usize,
    staged_count: usize,
    safe_included_count: usize,
    block_gas_used: u64,
    block_gas_limit: u64,
) -> bool

## intended behavior
仕様案:

```rust
requires true

ensures result == (
    committed_head >= previous_head
    && (block_gas_limit == 0 || block_gas_used <= block_gas_limit)
    && included_count != 0
    && included_count == staged_count
    && safe_included_count == included_count
)
```

要約:

- `committed_head` は `previous_head` 以上である。
- `block_gas_limit == 0` は無制限として扱う。
- 制限がある場合、`block_gas_used <= block_gas_limit` が必要。
- 空コミットは禁止する。
- included / staged / safe included の件数は一致する。
- 戻り値は上記条件の完全な論理積と一致する。

`specgen` 向けなら戻り値名は `result`。提示コードの `safe` 名を使う属性形式なら現行案で妥当。

## anchor
- git_commit: 4aed4c6b20d169ba2d31ba9c585394470dc69edf
- worktree_dirty: true
- source_hash: 318a0bf6163944ecb55fb9957b60e4b3150376ff426c3a59c24bad4fcd6a08d0
- semantic_hash: bf3de588a850213c5f19b3c3133b2f360e830849940559083c2e4fee1bcaeb2f
