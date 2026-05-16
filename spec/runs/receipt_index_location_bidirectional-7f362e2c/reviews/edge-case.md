# edge-case review

**Findings**

- Medium: [receipt_index.rs](/Users/0xhude/Desktop/ICP/Kasane/crates/verified-core/src/receipt_index.rs:16) は `tx_index_present=false && receipt_present=false && included_loc_present=false` で `true` になる。整合性述語なら妥当。ただし「included tx の存在証明」や「receipt がある」保証として使うと、全欠損入力で通過する。呼出側で存在を要求するなら `tx_index_present || receipt_present || included_loc_present` ではなく、用途に応じて `T && R && L` を precondition/別述語に分離するべき。

- Low: [p0_safety.rs](/Users/0xhude/Desktop/ICP/Kasane/crates/verified-core/tests/p0_safety.rs:28) は `index_matches_loc=false` と `receipt_matches_loc=false` の単独欠陥を固定していない。`loc_points_to_block_tx=false` はあるが、`I/M` 条件を削る mutation を捕捉しにくい。`all_present_index_mismatch`、`all_present_receipt_mismatch`、`none_absent_stale_flags` を追加すると境界が明確になる。

**実装評価**

式は次と等価:

```text
valid iff
  (!T && !R && !L)
  || (T && R && L && I && M && P)
```

部分存在は全て `false`。`I/M/P` は `T/R/L` 全欠損時に無視される。

**Verus**

`ensures result == ((!T && !R && !L) || (T && R && L && I && M && P))` が最も読みやすい。現行の含意連結でも証明対象としては問題ないが、vacuous truth が見えにくい。Verus 契約は簡約形を推奨。

検証コマンドは未実行。今回は読解レビューのみ。

