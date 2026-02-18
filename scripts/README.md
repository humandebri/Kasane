# scripts/README.md

このディレクトリの運用スクリプトの最短ガイドです。  
迷ったらまずこの順で実行してください。

## 前提
- 実行ディレクトリ: リポジトリルート（`IC-OP/`）
- 主な依存: `cargo`, `dfx`, `icp`, `node`, `npm`, `python`
- query 呼び出しは `dfx canister call --query ...` を使う

## よく使うコマンド

1. CI相当チェック（軽量）
```bash
scripts/ci-local.sh github
```

2. デプロイ前スモーク（標準）
```bash
scripts/predeploy_smoke.sh
```

3. ローカル統合スモーク（重め）
```bash
scripts/local_indexer_smoke.sh
```

4. Query専用スモーク
```bash
scripts/query_smoke.sh
```

## 用途別

### 事前確認・品質ゲート
- `scripts/ci-local.sh`: `github|smoke|all` の3モードで実行
- `scripts/predeploy_smoke.sh`: `cargo check` + wasm build + rpc/indexer smoke
- `scripts/run_rpc_compat_e2e.sh`: RPC互換E2Eテスト（`cargo test --test rpc_compat_e2e`）

### ローカル運用
- `scripts/dfx_local_clean_start.sh`: ローカル環境のクリーン起動補助
- `scripts/local_pruning_stage.sh`: pruning段階検証
- `scripts/local_indexer_fault_injection.sh`: indexer障害注入テスト

### playground
- `scripts/playground_manual_deploy.sh`: playground への手動デプロイ
- `scripts/playground_smoke.sh`: playground で Tx/RPC の一連確認
  - 送金系の追加確認は `FUNDED_ETH_PRIVKEY` を設定

### mainnet運用
- `scripts/mainnet/ic_mainnet_preflight.sh`: 本番前の最小チェック
- `scripts/mainnet/ic_mainnet_deploy.sh`: 本番デプロイ本体
- `scripts/mainnet/ic_mainnet_post_upgrade_smoke.sh`: デプロイ後の最小RPC確認
- `scripts/mainnet/mainnet_method_test.sh`: 本番メソッド検証（重い）
  - `MINING_IDLE_OBSERVE_SEC`: 冒頭の idle 観測秒数（既定: `6`）
  - `IDLE_MAX_CYCLE_DELTA`: idle 観測で許容する cycle 減少上限。`0` で閾値チェック無効（既定: `0`）

### prune運用
- `scripts/ops/apply_prune_policy.sh`: policy適用 + pruning有効化 + status確認
- `scripts/ops/tune_prune_max_ops.sh`: need_prune/error counters に基づく段階調整
- `scripts/ops/test_prune_ops_scripts.sh`: 上記2スクリプトのモック検証

## 主要環境変数（よく使うもの）
- `NETWORK`（例: `local`, `playground`）
- `CANISTER_NAME` / `CANISTER_ID`
- `ICP_IDENTITY_NAME`
- `RUN_DEPLOY`（`predeploy_smoke.sh` で local deploy を有効化）
- `RUN_INDEXER_SMOKE`（`predeploy_smoke.sh` で indexer smoke の有無）
- `RUN_POST_SMOKE`（`ic_mainnet_deploy.sh` で post smoke を有効化）

## 失敗時の切り分け
1. まず `scripts/query_smoke.sh` が通るか確認する  
2. 次に `scripts/rpc_compat_smoke.sh` を単体実行する  
3. 最後に `scripts/local_indexer_smoke.sh` を実行する  

重いスクリプトで失敗したときは、単体スクリプトに分解して再実行すると原因特定が速いです。
