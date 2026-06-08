# adversarial review

重大な指摘なし。

敵対入力観点:
- address不一致は false。
- `returned_success > 1` は false。
- 非overflow範囲でbaseだけ、inputだけ、replyだけを満たす過小合計gasは false。
- success/failureとgas_limitの不一致は false。
