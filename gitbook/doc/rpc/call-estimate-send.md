# Call, Estimate, Send

## TL;DR
- `eth_call` / `eth_estimateGas` は callObject制約あり。
- `eth_sendRawTransaction` は canister submit API委譲。

## メソッド
- `eth_call` -> `rpc_eth_call_object`
- `eth_estimateGas` -> `rpc_eth_estimate_gas_object`
- `eth_sendRawTransaction` -> `rpc_eth_send_raw_transaction`

## callObject制約
- 対応キー: `to/from/gas/gasPrice/value/data/nonce/maxFeePerGas/maxPriorityFeePerGas/chainId/type/accessList`
- `type=0x0` / `0x2` のみ
- feeパラメータ併用ルールあり

## 送信時の運用
- submit成功後に receipt `status` を必ず監視

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/README.md`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/handlers.ts`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-rpc/src/lib.rs`
