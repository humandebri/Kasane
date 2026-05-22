# adversarial review

**Findings**

- **High: `!next_present => true` が証明条件を破壊する。**  
  `next_boundary` が無い場合に無条件 `true` は危険。`retain == 0`、`head <= retain`、最新領域内の prune まで許可する。次境界が「prune 対象の終端証明」なら、未存在時は `false` が妥当。`true` にするなら、`!next_present` が常に安全を意味する明確な事前条件が必要。

- **Medium: 境界が inclusive なら off-by-one。**  
  `next_boundary > head - retain` なので、`next_boundary == head - retain` は prune 可になる。保持範囲が `head - retain..=head` なら境界上を削る危険がある。この場合は `next_boundary >= head - retain` にすべき。保持範囲が `(head - retain)..=head` なら現状でよいが、仕様で明記が必要。

- **Medium: `previous <= next_boundary` は弱い。**  
  順序検証だけなら `previous <= next_boundary` でよいが、同一境界を許容している。境界が区間開始点なら `previous < next_boundary` が自然。同値が許されると空区間・重複境界・進捗不能を隠す可能性がある。

**Verus 観点**

この実装は overflow 回避だけなら `head <= retain` で `head - retain` を守れている。ただし、主要安全性は bool 群の意味論に依存している。Verus では最低限、次を仕様化する必要がある。

```rust
requires
    previous_present ==> previous <= next_boundary,
    next_present ==> next_boundary <= head - retain, // または strict/非strictを仕様化
```

ただし現在の `!next_present => true` を維持するなら、`!next_present` が「後続区間なしなので prune しても保持範囲に影響しない」ことを別 invariant で証明する必要がある。証明できないなら `false` に倒すべき。

