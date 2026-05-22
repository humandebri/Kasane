# verus review

**所見**

中リスク: `head <= retain` が off-by-one の可能性あり。

`head` が「最新ブロック高」、ブロック高が `0` 始まり、`retain` が「保持するブロック数」なら、総ブロック数は `head + 1`。  
その場合、`head == retain` では `retain + 1` 個存在するため、`block == 0` は pruning 可能になる。

例:

```rust
head = 10
retain = 10
blocks = 0..=10 // 11 blocks
keep = 1..=10   // 10 blocks
prune = 0
```

現実装は `head <= retain` で `false` を返すため、1個余分に保持する。

修正方向:

```rust
if retain == 0 || head < retain {
    return false;
}
block <= head - retain
```

ただし、`retain == 0` を「pruning 無効」とする仕様なら現状でよい。`retain == 0` を「何も保持しない」と解釈するなら全く別仕様になる。

**Verus 観点**

現実装は `head <= retain` ガードにより `head - retain` の underflow は避けている。Verus でも証明しやすい形。

仕様化するなら、まず `head` の意味を固定する必要がある。

`head` が最新ブロック高、`retain` が保持数なら期待仕様は概ね:

```rust
ensures
    result == (
        retain != 0
        && head >= retain
        && block <= head - retain
    )
```

現実装をそのまま仕様化するなら:

```rust
ensures
    result == (
        retain != 0
        && head > retain
        && block <= head - retain
    )
```

差分は `head == retain` の1点。ここが仕様判断点。

**追加すべき境界テスト**

```rust
assert!(!block_is_prunable(10, 0, 0));
assert!(!block_is_prunable(5, 10, 0));
assert_eq!(block_is_prunable(10, 10, 0), /* 仕様次第 */);
assert!(block_is_prunable(11, 10, 1));
assert!(!block_is_prunable(11, 10, 2));
assert!(!block_is_prunable(11, 10, 12));
assert!(block_is_prunable(u64::MAX, 1, u64::MAX - 1));
```

結論: 算術安全性は問題なし。主要リスクは `head == retain` の境界仕様。最新高が0始まりなら、現実装は1ブロック余分に保持する可能性が高い。

