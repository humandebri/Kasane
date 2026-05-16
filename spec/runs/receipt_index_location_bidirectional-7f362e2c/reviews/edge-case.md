# edge-case review

**指摘**

- Medium: [receipt_index.rs](/Users/0xhude/Desktop/ICP/Kasane/crates/verified-core/src/receipt_index.rs:18) の Verus contract が実装式をそのまま複製している。Verus は「実装が意図仕様を満たす」ではなく「実装が同じ式を返す」だけを証明する形になる。意図を固定するなら、`result == ((!T && !R && !L) || (T && R && L && I && M && P))` のような正規化仕様、または `result ==> ...` / `!result ==> ...` の性質に分離するべき。

- Medium: [receipt_index.rs](/Users/0xhude/Desktop/ICP/Kasane/crates/verified-core/src/receipt_index.rs:24) は `tx_index_present=false && receipt_present=false && included_loc_present=false` で `true`。整合性述語なら妥当。ただし「receipt/index/location が存在する証明」として使うと全欠損で通過する。存在保証が必要な呼出側では `T && R && L` を別 precondition にするか、この関数を存在込みの述語に寄せる必要がある。

- Low: [p0_safety.rs](/Users/0xhude/Desktop/ICP/Kasane/crates/verified-core/tests/p0_safety.rs:28) は `index_matches_loc=false` と `receipt_matches_loc=false` の単独ケースを実行テストしていない。`all_present_index_mismatch` と `all_present_receipt_mismatch` を追加すると、リンク一致条件の削除 mutation を検出できる。

補足: 論理式自体は `(!T && !R && !L) || (T && R && L && I && M && P)` と等価。部分存在は通過しない。敵対的観点では、この関数は boolean 化された観測を信用するだけなので、adapter 側で全フラグを同一 canonical block/tx/location から生成する契約が必須。

