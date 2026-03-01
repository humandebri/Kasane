# Transaction & Receipt Methods

## TL;DR
- tx参照は `eth_tx_hash` 基準。
- receiptは `PossiblyPruned/Pruned` を明示的に返しうる。

## メソッド
- `eth_getTransactionByHash` -> `rpc_eth_get_transaction_by_eth_hash`
- `eth_getTransactionReceipt` -> `rpc_eth_get_transaction_receipt_with_status_by_eth_hash`

## 注意
- `tx_id` は内部キー、外部連携は `eth_tx_hash` を使う。
- 内部運用で tx_id を直接引く場合は `rpc_eth_get_transaction_receipt_with_status_by_tx_id` を使う。
- migration/corrupt時は `state unavailable` エラー。

## 根拠
- `tools/rpc-gateway/src/handlers.ts`
- `crates/ic-evm-rpc/src/lib.rs`
