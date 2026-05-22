# verus review

**所見**

重大な実装バグは見当たらない。全条件が `== 1` のときだけ `true` → raw flag predicate としては単純で検証しやすい。

注意点:

- `> 1` を `false` にする仕様なら妥当。  
  `points_to_tx` が「存在」ではなく「件数」なら、`2` は重複/破損なので `false` が正しい。  
  逆に upstream が truthy flag として `nonzero` を渡す契約なら、この実装は過剰に厳格。

- adversarial 入力では `u64::MAX`、`2`、未正規化値はすべて `false`。  
  これは安全側。ただし caller 側で `0/1` 正規化済みという前提があるなら、Verus の `requires` かコメントで契約化した方がよい。

- Verus 用途なら `ensures` を付ける価値がある。  
  実装と仕様を同一式で固定でき、後続証明が安定する。

```rust
pub fn staged_tx_is_current_pending_raw(
    ready_points_to_tx: u64,
    pending_meta_points_to_tx: u64,
    current_pending_points_to_tx: u64,
    tx_payload_present: u64,
    tx_not_marked_dropped: u64,
) -> (res: bool)
    ensures
        res == (
            ready_points_to_tx == 1
                && pending_meta_points_to_tx == 1
                && current_pending_points_to_tx == 1
                && tx_payload_present == 1
                && tx_not_marked_dropped == 1
        ),
{
    ready_points_to_tx == 1
        && pending_meta_points_to_tx == 1
        && current_pending_points_to_tx == 1
        && tx_payload_present == 1
        && tx_not_marked_dropped == 1
}
```

推奨テストは最小で十分:

- 全部 `1` → `true`
- 各引数だけ `0` → `false`
- 各引数だけ `2` → `false`
- 各引数だけ `u64::MAX` → `false`

結論: `1` が唯一の正規 true 値という契約なら実装は良い。Verus では `ensures` 追加が主な改善点。

