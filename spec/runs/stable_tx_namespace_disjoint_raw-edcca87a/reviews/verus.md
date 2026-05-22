# verus review

**Findings**

1. **仕様名と実装条件がずれる可能性**
   `namespace_disjoint` なら本質は「全IDが相異」だが、実装は「厳密昇順」まで要求している。  
   有効な相異IDでも順序が違えば `false` になる。順序が仕様なら名前/コメントで `ordered` を明示。相異だけが仕様なら `!=` の全ペア判定に変更。

2. **raw `u64` の有効範囲を検証していない**
   stable memory namespace が `MemoryId` 由来なら実ドメインは通常 `u8` 相当。  
   この関数は `1000,1001,...` でも `true` を返す。後段で `as u8` 等に落とす設計なら、Verus上の disjoint 証明が実装で破綻する。  
   修正方針: raw値の最大値制約を同じ述語に入れるか、入力型を有効ドメイン型にする。

3. **Verus仕様関数として使うなら `exec fn` では弱い**
   proof/spec 側で直接使う意図なら、`pub fn` ではなく `pub open spec fn` などに分離する方が明確。実行時チェックも必要なら exec wrapper に `ensures result == stable_tx_namespace_disjoint_spec(...)` を付ける。

**Edge Case**

- 隣接値が同一なら `false`。
- 非隣接の重複も厳密昇順が崩れるため `false`。
- `u64::MAX` 近辺でも加算なしなので overflow はない。
- 7個すべて相異でも昇順でなければ `false`。

**Adversarial**

攻撃入力としては「範囲外だが昇順」の raw値が最重要。  
例: `250, 251, 252, 253, 254, 255, 256` は `true` だが、後段の型変換次第で壊れる。

**結論**

実装は「厳密昇順の検査」としては正しい。  
「disjoint namespace 検査」としては、順序仕様と raw値ドメイン制約を明文化または実装に組み込む必要がある。

