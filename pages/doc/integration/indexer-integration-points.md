# Indexer Integration Points

## TL;DR
- pull起点は `export_blocks(cursor,max_bytes)`。
- logs補助取得は `rpc_eth_get_logs_paged`。
- prune前提で cursor運用を設計する。

## 根拠
- `crates/ic-evm-wrapper/evm_canister.did`
- `tools/indexer/README.md`
- `docs/specs/indexer-v1.md`
