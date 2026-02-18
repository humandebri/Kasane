# Logs

## TL;DR
- `eth_getLogs` は `rpc_eth_get_logs_paged` ベースの制限付き実装。
- 範囲・filter制約を超えると `-32005`。

## できること
- 単一address + `topic0`中心のログ取得

## できないこと
- `blockHash` 指定
- topics OR配列
- 複数address同時指定

## 代表エラー
- `logs.range_too_large`
- `logs.too_many_results`
- `UnsupportedFilter`

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-rpc/src/lib.rs`（`rpc_eth_get_logs_paged`）
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/handlers.ts`
