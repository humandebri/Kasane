# edge-case review

重大な指摘なし。

境界:
- `input_len = 0`、`reply_len = 0` ではbase gas下限だけが必要。
- `input_len = MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS` と `reply_len = MAX_ICP_QUERY_COMBINED_LEN_WITH_EXACT_GAS` は合計式がoverflowしない。
- `returned_success = 0` は `gas_limit < charged_gas`、`returned_success = 1` は `gas_limit >= charged_gas`。
