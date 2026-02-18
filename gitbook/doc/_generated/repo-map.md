# Repo Map（自動生成）

このページは、Kasane リポジトリの「どこに何があるか」を一次情報だけで整理したものです。

## TL;DR
- EVM canister本体は Rust workspace の `ic-evm-wrapper`。
- 実行ロジックは `evm-core`、永続化/定数は `evm-db`、RPC補助は `ic-evm-rpc`。
- 外部開発者向け入口は `tools/rpc-gateway`（HTTP JSON-RPC）と `crates/ic-evm-wrapper/evm_canister.did`（Candid）。
- indexer は `tools/indexer`（Postgres-first）。

## 主要ディレクトリ
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-wrapper`
  - canisterエントリポイント（`#[ic_cdk::query]` / `#[ic_cdk::update]`）
  - Candid公開定義（`evm_canister.did`）
- `/Users/0xhude/Desktop/ICP/Kasane/crates/evm-core`
  - tx decode / submit / produce / call / estimate 実装
  - EVM実行とfee計算（`revm_exec.rs`, `base_fee.rs`）
- `/Users/0xhude/Desktop/ICP/Kasane/crates/evm-db`
  - stable state、chain constants、runtime defaults、receipt/block/tx型
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-rpc`
  - wrapperから呼ばれる RPC補助ロジック（eth系参照/変換）
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway`
  - canister Candid API を Ethereum JSON-RPC 2.0 へ変換
- `/Users/0xhude/Desktop/ICP/Kasane/tools/indexer`
  - export API を pull して Postgresへ保存
- `/Users/0xhude/Desktop/ICP/Kasane/scripts`
  - smoke / predeploy / mainnet deploy 補助
- `/Users/0xhude/Desktop/ICP/Kasane/docs`
  - 運用runbook・仕様メモ（一次情報として参照可能）

## エントリポイント
- canister build/runtime
  - `/Users/0xhude/Desktop/ICP/Kasane/dfx.json` の `canisters.evm_canister`
  - `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-wrapper/src/lib.rs`
- gateway runtime
  - `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/main.ts`
  - `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/server.ts`
- indexer runtime
  - `/Users/0xhude/Desktop/ICP/Kasane/tools/indexer/src/main.ts`

## 依存関係の大枠
- Rust workspace members
  - `/Users/0xhude/Desktop/ICP/Kasane/Cargo.toml` `[workspace].members`
- JSON-RPC層の実装責務
  - Gateway: request/responseと制限
  - canister: state/実行/永続化

## 非対象（本GitBookで扱わない）
- ノード運営（validator/sequencer/full node運用手順）
- `vendor/` 配下の上流ライブラリ内部詳細

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/Cargo.toml`（workspace members）
- `/Users/0xhude/Desktop/ICP/Kasane/dfx.json`（canister package/candid）
- `/Users/0xhude/Desktop/ICP/Kasane/icp.yaml`（deploy recipe）
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-wrapper/src/lib.rs`（canister entrypoint）
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/main.ts`（gateway entrypoint）
- `/Users/0xhude/Desktop/ICP/Kasane/tools/indexer/README.md`（indexer責務）
