# Troubleshooting Guide

## TL;DR
- まず「入力不正」「state unavailable」「pruned」のどれかを切り分ける。
- submitとexecuteを分けて観測する。

## 代表エラーと対処
- `invalid params (-32602)`
  - 原因: address/hash長不正、blockTag不正、callObject制約違反
  - 対処: 引数hex長・対応キーを再確認
- `state unavailable (-32000)`
  - 原因: migration中、critical_corrupt、実行失敗
  - 対処: `get_ops_status` を確認
- `resource not found (-32001)`
  - 原因: pruned範囲、対象なし
  - 対処: indexer側履歴を参照
- `limit exceeded (-32005)`
  - 原因: logs範囲過大/件数過多
  - 対処: ブロック範囲分割

## 落とし穴
- `eth_getLogs` を大範囲一発で引く
- 監視なしで再送して nonce競合を増やす

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/handlers.ts`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/README.md`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-wrapper/src/lib.rs`（`get_ops_status`）
