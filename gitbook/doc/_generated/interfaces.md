# Interfaces（自動生成）

## TL;DR
- 外部I/Fは `Candid`（canister直呼び）と `HTTP JSON-RPC`（gateway）の2系統。
- JSON-RPCは Ethereum完全互換ではなく、制限付き実装。
- `eth_sendRawTransaction` は submit成功≠実行成功。`eth_getTransactionReceipt.status` を成功条件にする。

## 1. Candid API（公開service）
公開定義: `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-wrapper/evm_canister.did`

### 主要query
- `rpc_eth_chain_id`
- `rpc_eth_block_number`
- `rpc_eth_get_block_by_number`
- `rpc_eth_get_block_by_number_with_status`
- `rpc_eth_get_transaction_by_eth_hash`
- `rpc_eth_get_transaction_by_tx_id`
- `rpc_eth_get_transaction_receipt_by_eth_hash`
- `rpc_eth_get_transaction_receipt_with_status`
- `rpc_eth_get_balance`
- `rpc_eth_get_code`
- `rpc_eth_get_storage_at`
- `rpc_eth_call_object`
- `rpc_eth_call_rawtx`
- `rpc_eth_estimate_gas_object`
- `rpc_eth_get_logs_paged`
- `expected_nonce_by_address`
- `get_receipt`
- `get_pending`
- `export_blocks`
- `get_ops_status`
- `health`
- `metrics`

### 主要update
- `rpc_eth_send_raw_transaction`
- `submit_ic_tx`
- `set_block_gas_limit`
- `set_instruction_soft_limit`
- `set_prune_policy`
- `set_pruning_enabled`
- `set_log_filter`
- `prune_blocks`

## 2. Gateway JSON-RPC
実装: `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/handlers.ts` の `handleRpc` switch

### 実装済みメソッド
- `web3_clientVersion`
- `net_version`
- `eth_chainId`
- `eth_blockNumber`
- `eth_gasPrice`
- `eth_syncing`
- `eth_getBlockByNumber`
- `eth_getTransactionByHash`
- `eth_getTransactionReceipt`
- `eth_getBalance`
- `eth_getTransactionCount`
- `eth_getCode`
- `eth_getStorageAt`
- `eth_getLogs`
- `eth_call`
- `eth_estimateGas`
- `eth_sendRawTransaction`

### 未対応（READMEの互換表）
- `eth_getBlockByHash`
- `eth_getTransactionByBlockHashAndIndex`
- `eth_getTransactionByBlockNumberAndIndex`
- `eth_getBlockTransactionCountByHash`
- `eth_getBlockTransactionCountByNumber`
- `eth_feeHistory`
- `eth_maxPriorityFeePerGas`
- `eth_newFilter`
- `eth_getFilterChanges`
- `eth_uninstallFilter`
- `eth_subscribe`
- `eth_unsubscribe`
- `eth_pendingTransactions`

## 3. JSON-RPC制約（抜粋）
- `eth_getBalance` は `latest` 系 blockTagのみ
- `eth_getTransactionCount` は `latest/pending/safe/finalized` のみ
- `eth_getCode` は `latest` 系のみ
- `eth_getStorageAt` は `latest` 系のみ
- `eth_call` / `eth_estimateGas` は `latest` 系のみ
- `eth_getLogs` は制限あり
  - `blockHash` 非対応
  - `address` は単一のみ
  - `topics` は `topics[0]` 中心（OR配列非対応）

## 4. 返却・識別子の注意
- canister内部識別子: `tx_id`
- Ethereum互換識別子: `eth_tx_hash`
- Gateway `eth_sendRawTransaction` は canister戻り `tx_id` から `eth_tx_hash` を解決して返す。

## 5. 型境界（代表）
- `RpcCallObjectView`（Candid）
- `EthBlockView` / `EthTxView` / `EthReceiptView`
- `RpcBlockLookupView`（`NotFound` / `Found` / `Pruned`）
- `RpcReceiptLookupView`（`NotFound` / `Found` / `PossiblyPruned` / `Pruned`）

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-wrapper/evm_canister.did`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/handlers.ts`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/README.md`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-wrapper/src/lib.rs`（`rpc_eth_send_raw_transaction`）
