# draft: block_is_prunable-04224fd7

## inferred behavior
pub fn block_is_prunable(head: u64, retain: u64, block: u64) -> bool

## intended behavior
仕様ドラフト候補:

```rust
requires true
ensures result == (retain != 0 && head > retain && block <= head - retain)
```

受入基準:

- `retain == 0` の場合、常に `false`
- `head <= retain` の場合、常に `false`
- `retain > 0 && head > retain` の場合、`block <= head - retain` と同値
- `head - retain` は `head > retain` 分岐内でのみ評価され、u64 underflow は発生しない

## anchor
- git_commit: a3bc9781ec94f42ff9edf5612aebd4f4532e69f0
- worktree_dirty: false
- source_hash: 04224fd721eb346c0179a3a897953c3f7f5bed4a2f5c8204cf15ddab000034cf
- semantic_hash: c76192f60877bd0b3b3fecf672e540724f0ef2b2a562bf86319e8b669fea139a
