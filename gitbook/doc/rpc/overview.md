# RPC Overview

## TL;DR
- Gateway は canister Candid API を JSON-RPC 2.0 に変換する層です。
- 互換は制限付きで、未対応メソッドがあります。
- 入力不正は主に `-32602`、状態不整合は `-32000/-32001`。

## 対応範囲
- `web3_*`, `net_*`, `eth_*` の一部
- 実装メソッドは `handleRpc` の switch が正本

## できること / できないこと
- できること: 基本参照、call、estimate、raw tx投入
- できないこと: filter/ws/pending完全互換

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/handlers.ts`（`handleRpc`）
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/README.md`
