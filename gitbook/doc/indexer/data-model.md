# Data Model

## TL;DR
- indexerは Postgres-first。
- `txs`, `receipts`, `token_transfers`, `ops_metrics_samples` などを保持。

## 主要ポイント
- `receipt_status` を `txs` に保持
- token transfer は receipt logsから抽出

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/tools/indexer/README.md`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/indexer/src/db.ts`
