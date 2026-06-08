# edge-case review

重大な指摘なし。

境界:
- `target_len = 1` と `MAX_PRINCIPAL_LEN` は他条件が有効なら true。
- `target_len = 0` と `MAX_PRINCIPAL_LEN + 1` は false。
- `method_len = 1` と `MAX_QUERY_METHOD_LEN` は他条件が有効なら true。
- `method_len = 0` と `MAX_QUERY_METHOD_LEN + 1` は false。
