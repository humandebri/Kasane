# Indexer Integration Points

## TL;DR
- pull起点は `export_blocks(cursor,max_bytes)`。
- logs補助取得は `rpc_eth_get_logs_paged`。
- prune前提で cursor運用を設計する。

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-wrapper/evm_canister.did`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/indexer/README.md`
- `/Users/0xhude/Desktop/ICP/Kasane/docs/specs/indexer-v1.md`
