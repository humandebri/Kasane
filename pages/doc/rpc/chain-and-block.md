# Chain & Block Methods

## TL;DR
- `eth_chainId`, `net_version`, `eth_blockNumber`, `eth_getBlockByNumber` を提供。
- `eth_getBlockByNumber` は prune範囲で `-32001`。

## メソッド
- `eth_chainId` -> canister `rpc_eth_chain_id`
- `net_version` -> `rpc_eth_chain_id` を10進文字列化
- `eth_blockNumber` -> canister `rpc_eth_block_number`
- `eth_getBlockByNumber` -> canister `rpc_eth_get_block_by_number_with_status`

## blockTag制約
- `latest/pending/safe/finalized` は head扱い
- prune済みは `resource not found`

## 根拠
- `tools/rpc-gateway/src/handlers.ts`
- `crates/ic-evm-rpc/src/lib.rs`
