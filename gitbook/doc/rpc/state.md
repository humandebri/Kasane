# State Methods

## TL;DR
- `eth_getBalance`, `eth_getTransactionCount`, `eth_getCode`, `eth_getStorageAt` を提供。
- 多くが `latest` 系blockTagのみ。

## メソッド
- `eth_getBalance` -> `rpc_eth_get_balance`
- `eth_getTransactionCount` -> `expected_nonce_by_address`
- `eth_getCode` -> `rpc_eth_get_code`
- `eth_getStorageAt` -> `rpc_eth_get_storage_at`

## 制約
- balance/code/storage: `latest` 系のみ
- tx count: `latest/pending/safe/finalized` のみ

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/handlers.ts`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-wrapper/evm_canister.did`
