# implementation review

**Findings**

- Medium: [query.rs:18](/Users/0xhude/Desktop/ICP/Kasane/crates/verified-core/src/prune_safety/query.rs:18) 以降の述語は「不正な `Ok` / `Pruned` を拒否」するだけで、「返すべき結果」を要求しない。  
  例: `prune_query_observation_safe_raw(1, 8, 10, 0, 0, 0)` は `true`。`block_number <= pruned_before` なのに `Pruned` なしでも通る。query/export 実装契約が「pruned 範囲は必ず `Pruned`」なら弱すぎる。  
  修正方向: `boundary_present != 0 && block_number <= pruned_before` の場合に `returned_pruned != 0` を要求する、または関数名/コメントを「one-way safety」に限定する。

- Medium: [query.rs:18](/Users/0xhude/Desktop/ICP/Kasane/crates/verified-core/src/prune_safety/query.rs:18) は `retained != 0` でも `returned_ok == 0` を許容する。  
  例: `prune_query_observation_safe_raw(1, 12, 10, 1, 0, 0)` は `true`。`retained` が「対象データが存在する」意味なら、NotFound 相当を許すため実装バグを隠す。  
  修正方向: `retained` が存在性なら `retained != 0 ==> returned_ok != 0` を追加。単なる保持範囲なら、名前を `in_retained_range` などに寄せる。

- Low / Verus: raw `u64` フラグ API は不整合入力を呼び手に許す。`boundary_present = 0` なら `pruned_before` は無視され、`boundary_present = 1, pruned_before = u64::MAX` は全ブロック pruned 扱いになる。  
  Verus 証明用なら `bool` / `Option` 相当の spec predicate を主にし、raw 版は adapter に限定した方が VC と誤用が減る。

**結論**

クラッシュ・overflow・短絡評価の問題はない。  
ただし実装検証用の完全契約としては弱い。現状は「矛盾する観測を拒否する negative predicate」であり、「正しい query 応答を強制する predicate」ではない。テストもこの弱さを明示する true ケースを追加するか、契約を強化して rejection ケースに変えるべき。

