# draft: no_reorg_append_only_raw-f8bac44c

## inferred behavior
pub fn no_reorg_append_only_raw(
    previous_head: u64,
    committed_head: u64,
    parent_points_to_previous_head: u64,
    previous_blocks_unchanged: u64,
    previous_receipts_unchanged: u64,
    previous_indexes_unchanged: u64,
) -> bool

## intended behavior
仕様ドラフト候補:

```text
Target: no_reorg_append_only_raw

Intent:
前回headから1ブロックだけ進み、既存ブロック・receipt・indexが不変で、親参照も前回headを指す場合だけtrueを返す。

Preconditions:
なし。previous_head == u64::MAX は拒否条件として扱う。

Postconditions:
result ==
    previous_head < u64::MAX
    && committed_head == previous_head + 1
    && parent_points_to_previous_head == 1
    && previous_blocks_unchanged == 1
    && previous_receipts_unchanged == 1
    && previous_indexes_unchanged == 1

Acceptance criteria:
- 1ブロックだけappendされ、親参照と既存データ不変性が全て成立する場合true。
- headが進まない、飛ぶ、overflow境界、親不一致、既存blocks/receipts/indexesのいずれか変更の場合false。
```

## anchor
- git_commit: 45c236f431ea13639e1ce09e51a6e84f7b627d28
- worktree_dirty: true
- source_hash: f8bac44c1efdfa27b0b550c2955e02a134ecb3fd90e1d4a448b95c40520fde58
- semantic_hash: 901f8070f0a2be75c95df3e3042c20ff116174a9e47cd1a0277fbe7812d4db5d
