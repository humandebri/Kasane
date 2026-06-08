# adversarial review

重大な指摘なし。

敵対入力観点:
- `kind = ICP_QUERY_KIND_UPDATE_RESERVED` と未知kindは false。
- target/methodの0長と上限超過は false。
- trailing data、欠落arg、非UTF-8 methodは対応flagが `1` にならない限り false。
- flagに `2` 以上を入れても true にならない。
