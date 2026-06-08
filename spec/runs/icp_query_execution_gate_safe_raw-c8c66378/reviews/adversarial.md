# adversarial review

重大な指摘なし。

確認結果:
- 実装は完全な純粋述語で、副作用・panic・overflow はない。
- `u64` の非正規値はすべて fail closed になる。例: `mode_allows_external = 2`、`value_is_zero = 42` は `false`。
- Verus の `ensures valid == (...)` は実装と一致している。
- `calls_before == 0` も敵対入力に対して明確。`u64::MAX` は `false`。

注意点:
- この仕様は「実装と同じ式を返す」ことだけを保証する。`mode_allows_external` / `value_is_zero` / `parsed_input` が 0/1 に正規化済みであることは保証しない。
- 呼び出し側が「非ゼロなら true」と解釈している場合、この関数は意図より厳しい。現名の `*_raw` なら現挙動で妥当。
- テストを追加するなら、8通りの 0/1 組合せと、各フラグの非正規値 rejection を見る表駆動テストで十分。
