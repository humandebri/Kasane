# Data Model

## TL;DR
- indexerは Postgres-first。
- `txs`, `receipts`, `token_transfers`, `ops_metrics_samples` などを保持。

## 主要ポイント
- `receipt_status` を `txs` に保持
- token transfer は receipt logsから抽出

## 根拠
- `tools/indexer/README.md`
- `tools/indexer/src/db.ts`
