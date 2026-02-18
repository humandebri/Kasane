# Security Guide

## TL;DR
- 署名鍵管理、chain id一致、receipt監視が最低ライン。
- 低fee設定・nonce運用ミス・prune未考慮が典型的事故要因。

## 安全な使い方
- chain id固定（`4801360`）
- nonceは毎回取得
- `eth_sendRawTransaction` 後に receipt判定
- gateway制限値を過剰緩和しない

## 危険な落とし穴
- `status=0x0` を見落として成功扱い
- 誤chain idで署名したtxを再送し続ける
- prune後参照を永続前提にする

## 推奨設定
- `RPC_GATEWAY_MAX_BATCH_LEN` を既定範囲で維持
- `INDEXER_CHAIN_ID` を実ネットワークと一致
- `INDEXER_MAX_SEGMENT` を canister仕様と一致

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/config.ts`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/indexer/src/config.ts`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/README.md`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/evm-core/tests/phase1_eth_decode.rs`
