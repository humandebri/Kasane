# implementation review

所見: 重大な実装バグは見えない。

この述語は次を正確に表している:

- `block_number <= pruned_through` → pruned 応答のみ許可
- `pruned_through < block_number` → pruned 応答禁止、`retained == returned_ok`
- 各観測値は 0/1 に制限
- `returned_ok == 1 && returned_pruned == 1` は到達不能

注意点は1つ。

`pruned_through < block_number` 側で `retained == 0 && returned_ok == 0` を許可している。  
「未prune領域でもブロックが存在しない可能性がある」仕様なら妥当。  
「未prune領域の対象 block は必ず保持済み」の仕様なら弱すぎるため、`retained == 1 && returned_ok == 1` が必要。

Verus観点では証明しやすい形。算術加算がないため overflow リスクなし。  
ただし、証明で使うなら disjunction より implication 形式のほうが補題適用しやすい:

```rust
retained <= 1
    && returned_ok <= 1
    && returned_pruned <= 1
    && (block_number <= pruned_through ==> retained == 0 && returned_ok == 0 && returned_pruned == 1)
    && (pruned_through < block_number ==> returned_pruned == 0 && retained == returned_ok)
```

最小テストは境界だけで足りる:

- `block_number == pruned_through` は pruned 側
- `block_number == pruned_through + 1` は retained 側
- retained 側の `(0,0,0)` を許可するか拒否するか
- `returned_ok=1, returned_pruned=1` は常に拒否
- 各 count が `2` の場合は拒否

