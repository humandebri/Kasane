# verus review

前提: 各フィールドが「残存している対象」を表す bool なら実装は妥当。

**Findings**

指摘なし。実装は `complete == 全対象が false` を直接表していて、panic・overflow・副作用・短絡順依存はない。

**Edge Case**

- 全 false → `true`
- 1つでも true → `false`
- 入力が不正に構築された場合は防げない。検証対象は `PruneTxCleanupInput` の生成側。

**Adversarial**

この関数単体に迂回余地はない。攻撃面は「実際には残存しているのに対応 field が false になる」経路。DB/インデックス/trace/receipt の存在確認ロジック側を重点確認するべき。

**Verus**

Verus で証明対象にするなら、実装に仕様を固定するとよい。

```rust
pub fn prune_tx_cleanup_complete(input: PruneTxCleanupInput) -> (res: bool)
    ensures
        res == (
            !input.tx_store
            && !input.receipt
            && !input.tx_index
            && !input.internal_traces
            && !input.tx_loc
            && !input.seen_tx
        ),
{
    !input.tx_store
        && !input.receipt
        && !input.tx_index
        && !input.internal_traces
        && !input.tx_loc
        && !input.seen_tx
}
```

再利用する仕様なら `open spec fn` に切り出し、実装は `ensures res == spec(input)` にする。テストは全 false と各 field 単独 true の 7 ケースで十分。

