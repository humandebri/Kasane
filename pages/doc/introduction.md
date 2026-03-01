# What is Kasane

## TL;DR
Kasane は、ICP canister 上で EVM実行を提供するプロジェクトです。  
公開導線は「canister Candid API」と「Gateway JSON-RPC」です。  
JSON-RPCは Ethereum完全互換ではなく、開発用途の制限付き互換です。  
互換ターゲットは Ethereum JSON-RPC + EVM実行意味論で、OP/Superchain互換は非目標です。

## できること
- Ethereum風 JSON-RPC で基本的な参照/送信を行う
- canister query/update で直接EVM機能を呼ぶ
- indexer で `export_blocks` を pull し、Postgresへ保持する

## できないこと（現行実装）
- Ethereumノード完全互換（mempool/filter/WebSocket full）
- node運用向けワークフローの提供（本書対象外）

## 互換の立ち位置
- 明示対象: Ethereum JSON-RPC 互換（制限付き）
- 非目標: OP-stack / Superchain 互換
- 注意: `eth_sendRawTransaction` の成功は submit成功のみ。実行成功は receipt `status=0x1` で判定

## 読者対象
- EVM dApp開発者
- スマートコントラクト開発者
- バックエンド統合開発者
- indexer開発者

## 根拠
- `README.md`（運用サマリ、互換方針）
- `tools/rpc-gateway/README.md`（Gatewayの互換表）
- `crates/ic-evm-wrapper/evm_canister.did`（公開I/F）
