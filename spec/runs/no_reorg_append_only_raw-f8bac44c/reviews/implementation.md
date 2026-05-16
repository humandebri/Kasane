# implementation review

**Findings**

- **High: caller-supplied flags are forgeable.**  
  `parent_points_to_previous_head` / `previous_*_unchanged` が外部入力なら、攻撃者は全部 `1` を渡せる。実状態検証ではなく「検証済み事実の集約」だけになっている。修正方向: raw関数は非公開にし、公開側で blocks / receipts / indexes から各条件を計算する。

- **Medium: append-only が 1 block 追加に限定されている。**  
  `committed_head == previous_head + 1` なので、複数blockを一括commitする正常系を拒否する。仕様が「単一block commit」なら関数名に含める。一般の no-reorg append-only なら `committed_head >= previous_head` と祖先チェーン検証が必要。

- **Medium: genesis / empty chain が表現できない可能性。**  
  `previous_head` が必須で、`u64::MAX` を拒否する。空チェーンの番兵に `u64::MAX` を使う設計なら初回commitは常に失敗する。初回commit用の別仕様が必要。

- **Low: `u64` フラグは仕様が弱い。**  
  `== 1` で実質boolだが、型が `u64` なので Verus 上も意味が濁る。実装関数なら `bool` にする方が契約が明確。回路/外部証明由来で `u64` が必要なら、境界で `flag == 1` をbool化する。

**Verus観点**

`previous_head < u64::MAX && committed_head == previous_head + 1` は Rust 実行時には短絡評価で overflow を避ける。ただし Verus が算術overflow義務をどこまで前件から推論するかに依存する。検証が不安定なら、明示的に分ける方が堅い。

```rust
previous_head < u64::MAX
    && committed_head == previous_head + 1
```

は以下の形にすると証明条件が明確になる。

```rust
if previous_head == u64::MAX {
    false
} else {
    committed_head == previous_head + 1
        && parent_points_to_previous_head == 1
        && previous_blocks_unchanged == 1
        && previous_receipts_unchanged == 1
        && previous_indexes_unchanged == 1
}
```

総評: raw predicate としては単純でよい。ただし安全性は「各flagを誰がどう算出したか」に完全依存する。公開APIや状態遷移ガードとして直接使うなら不足。

