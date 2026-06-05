# adversarial review

重大な指摘なし。

敵対入力観点:
- `kind = 0` は false。
- `kind = ICP_QUERY_KIND_UPDATE_RESERVED` は true。
- `kind = ICP_QUERY_KIND_UPDATE_RESERVED + 1` と `u64::MAX` は false。
