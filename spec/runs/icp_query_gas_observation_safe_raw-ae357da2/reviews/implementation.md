# implementation review

重大な指摘なし。

確認結果:
- gas観測はICP query precompile addressを必須にする。
- `returned_success` は0/1だけを受理する。
- 保守的な非overflow範囲では `base + input * 16 + reply * 8` の合計下限を要求する。
- 成功時は `gas_limit >= charged_gas`、失敗時は `gas_limit < charged_gas` を要求する。

注意点:
- 範囲外の巨大長はRust overflowを避けるため合計式の対象外である。実装側はsaturating課金なので、現実入力範囲ではPBT/async testで補完する。
