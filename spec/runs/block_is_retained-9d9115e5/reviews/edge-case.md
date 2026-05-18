# edge-case review

**Findings**

1. **境界 `head == retain` が仕様次第で危険**
   - 現実装は `head <= retain` で `block <= head` を全部保持する。
   - 例: `head=3, retain=3` → `block=0` も `true`。
   - `retain` が「保持ブロック数」なら 3 個でなく 4 個保持する。条件は `head < retain` 側が自然。
   - `retain` が「head からの距離」なら `block > head - retain` が逆に 1 個落とす可能性がある。`>=` が自然。

2. **`retain == 0` の意味が要明文化**
   - 現実装は `retain == 0` を「全保持」と解釈する。
   - 「保持なし」や「head のみ保持」と読む設計もあり得るため、仕様・テスト名で固定すべき。

3. **Verus向けには off-by-one 仕様を先に固定すべき**
   - 算術安全性は良い。`head <= retain` 分岐により `head - retain` の underflow は回避される。
   - ただし証明対象の predicate が曖昧だと、誤仕様を正しく証明するだけになる。

**推奨仕様例**

`retain` が「保持する最新ブロック数」なら:

```rust
block <= head && (retain == 0 || block + retain > head)
```

ただし `block + retain` は overflow し得るため、Verus/Rust実装では差分形が良い:

```rust
if block > head {
    return false;
}
if retain == 0 {
    return true;
}
if head < retain {
    return true;
}
block > head - retain
```

この場合、`head == retain` では `block=0` は保持されない。`head=3, retain=3` → `1,2,3` を保持。

`retain` が「head から retain 以内の距離」なら最後は:

```rust
block >= head - retain
```

現実装は安全性面では大きな問題なし。主リスクは `retain` の意味と境界の off-by-one。

