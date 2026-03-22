# scripts/README.md

English version: [./README.md](./README.md)


このディレクトリの運用スクリプトの最短ガイドです。  
迷ったらまずこの順で実行してください。

## 前提
- 実行ディレクトリ: リポジトリルート（`Kasane/`）
- 主な依存: `cargo`, `icp`, `node`, `npm`, `python`
- query 呼び出しは `dfx canister call --query ...` を使う
- query 系だけは現時点で `dfx` を維持し、非 query 系は `icp` に統一する
- ローカル検証は場当たり的な local deploy より PocketIC を優先する
- 既にローカルに PocketIC バイナリがある場合は、そのバイナリを優先して使う

## よく使うコマンド

1. CI相当チェック（軽量）
```bash
CI_LOCAL_MODE=github scripts/ci-local.sh
```

2. デプロイ前スモーク（標準, PocketIC）
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

5. Wasm 依存可視化（Phase 0.5）
```bash
scripts/profile_wasm_deps.sh --package ic-evm-gateway
```

6. Precompile 比率計測
```bash
CANISTER_NAME_OR_ID=<id> \
WORKLOAD_CMD='scripts/playground_smoke.sh' \
scripts/measure_precompile_ratio.sh
```

## 用途別

### 事前確認・品質ゲート
- `scripts/ci-local.sh`: `CI_LOCAL_MODE=<mode>` で `github|smoke|all` の3モードを切り替える
- `scripts/ci_github_equivalent.sh`: `.github/workflows/ci.yml` と `scripts/ci-local.sh` が共有する GitHub 相当チェックの単一ソース
  - Rust の標準品質ゲートとして `cargo fmt --all -- --check` と `cargo clippy --workspace --all-targets --all-features -- -D warnings` を含む
- `scripts/check_gateway_api_compat_baseline.sh`: Gateway API compatibility baseline の破壊変更を検知（`--update` でベースライン更新）
- `scripts/check_gateway_matrix_sync.sh`: `tools/rpc-gateway/README.md` の互換マトリクス行が `tools/rpc-gateway/package.json` のバージョン系列と一致するか検証
- `scripts/check_precompile_feature_isolation.sh`: `ic-evm-core` の既定 wasm build に BLS/KZG backend crate（`ark-bls12-381`, `c-kzg`, `blst`）が流入していないか検証
- `scripts/predeploy_smoke.sh`: `cargo check` + wasm build + PocketIC RPC互換E2E（任意で indexer smoke）
- `scripts/run_rpc_compat_e2e.sh`: RPC互換E2Eテスト（`cargo test --test rpc_compat_e2e`）
  - Rust の E2E テストは `tools/wrapper-vite/contracts/out/` の Foundry artifact をコンパイル時に読むため、この script は先に `forge build` を実行する
  - PocketIC が localhost (`127.0.0.1`) に bind できる必要がある。制限付き sandbox ではテスト本体の前に失敗することがある
- `scripts/prepare_ci_icrc1_ledger_wasm.sh`: 共通 ledger artifact helper 経由で、レポ同梱の official ledger wasm `third_party/dfinity/ledger-suite-icrc-2026-03-09/ic-icrc1-ledger.wasm` を `ICP_LEDGER_WASM` として export する。`LEDGER_RELEASE=latest` は拒否し、local ledger smoke は `ledger.did` を `${LEDGER_CACHE_DIR}/<release>/ledger.did` に cache する
- `scripts/profile_wasm_deps.sh`: wasm の依存サイズ可視化（`twiggy top/dominators`、nightly があれば `cargo +nightly bloat -Z build-std`、`cargo tree -e features -i <crate>` を保存）
  - 出力先既定: `docs/ops/reports/wasm-deps-<package>-<timestamp>/`
  - `--compare <前回の出力ディレクトリ>` で before/after 比較表（bytes + instruction estimate）を生成

### rpc-gateway ドキュメント言語ポリシー
- `tools/rpc-gateway/README.md` は英語正本
- 日本語補助は `tools/rpc-gateway/README.ja.md`（`ops/`, `smoke/`, `contracts/` も同様）

### ローカル運用
- `scripts/icp_local_clean_start.sh`: managed local network（`icp network`）のクリーン起動補助
- `scripts/local_pruning_stage.sh`: pruning段階検証
- `scripts/local_indexer_fault_injection.sh`: indexer障害注入テスト
- `scripts/measure_precompile_ratio.sh`: ワークロード再生後に `get_precompile_profile` を集計し、固定 precompile ratio の候補を算出する
  - 課金判断は IC instruction counter を正とし、wall-clock 時間は使わない
  - 開始前に `clear_precompile_profile` の成功を検証し、失敗時は古い profile を混ぜずに即停止する
  - 固定 target を使う場合は `TARGET_GAS_PER_INSTRUCTION` を設定
  - 参照 precompile から導出する場合は `REFERENCE_PRECOMPILE_ADDRESS` と `REFERENCE_TARGET_GAS` を設定
  - 重い `modexp` を参照にする例: `REFERENCE_PRECOMPILE_ADDRESS=0x0000000000000000000000000000000000000005 REFERENCE_TARGET_GAS=3000000`
  - canister の ratio state は変更しない。採用する場合は fixed ratio をコードに反映して再デプロイする
- `scripts/run_precompile_profile_e2e.sh`: 最新 gateway wasm を build して PocketIC で precompile profile を計測し、既定対象（`ecrecover`, `blake2f`, `modexp`）の `get_precompile_profile` を出力
  - 出力は `total_instructions` / `avg_instructions` を基準に読む。PocketIC の wall-clock は判断材料にしない
  - PocketIC が localhost (`127.0.0.1`) に bind できる必要がある。sandbox で bind error が出る場合は通常のターミナル環境で再実行する
  - `PRECOMPILE_PROFILE_TARGETS=ecrecover,blake2f,modexp,modexp_heavy` のようにカンマ区切りで対象を上書き可能
  - `modexp_heavy` を使うと、既定の軽量 `modexp` より大きい 32-byte fixture を計測できる
  - `p256` は現行 execution spec で RIP-7212 precompile が有効な場合のみ対象にできる
  - この script は `ic-evm-gateway` を `precompile-profile-admin` feature 付きで build する。既定の canister build では計測専用 API は公開しない
  - postprocess 後の wasm は candid metadata / endpoint validation に `crates/ic-evm-gateway/evm_canister_precompile_profile_admin.did` を使う
  - cleanup 方針メモは `docs/ops/precompile_profile_cleanup.md`
  - 保存 JSON の例（`PRECOMPILE_PROFILE_JSON_PATH=/tmp/precompile_profile.json`）:
```json
{
  "runs": 30,
  "targets": ["ecrecover", "blake2f", "modexp"],
  "entries": [
    {
      "name": "ecrecover",
      "address": "0x0000000000000000000000000000000000000001",
      "calls": 30,
      "avg_instructions": 212777,
      "max_instructions": 212900,
      "avg_extra_gas": 2128,
      "max_extra_gas": 2129
    },
    {
      "name": "blake2f",
      "address": "0x0000000000000000000000000000000000000009",
      "calls": 30,
      "avg_instructions": 55307225,
      "max_instructions": 55307235,
      "avg_extra_gas": 553073,
      "max_extra_gas": 553073
    },
    {
      "name": "modexp",
      "address": "0x0000000000000000000000000000000000000005",
      "calls": 30,
      "avg_instructions": 31495,
      "max_instructions": 31615,
      "avg_extra_gas": 315,
      "max_extra_gas": 317
    }
  ]
}
```

### playground
- `scripts/playground_manual_deploy.sh`: playground への手動デプロイ
- `scripts/playground_smoke.sh`: playground で Tx/RPC の一連確認
  - 送金系の追加確認は `FUNDED_ETH_PRIVKEY` を設定

### mainnet運用
- `scripts/mainnet/ic_mainnet_preflight.sh`: 本番前の最小チェック
  - `CANISTER_NAME` は既定で `evm_canister`。`wrap_canister` はこの script の対象外
  - 既定の cycles 下限は `MIN_CYCLES=2000000000000`
- `scripts/mainnet/ic_mainnet_deploy.sh`: 本番デプロイ本体
  - 既定 build では `precompile-profile-admin` feature を有効化しない
  - precompile ratio の計測は deploy 前に `scripts/run_precompile_profile_e2e.sh` / `scripts/measure_precompile_ratio.sh` で実施し、既定の fixed ratio `1/100` を見直す場合は再デプロイする
  - `MODE=upgrade` でも `WRAP_CANISTER_ID` と `EVM_WRAP_FACTORY` が必須
  - instruction soft limit を install / upgrade で上書きしたい場合だけ `QUERY_INSTRUCTION_SOFT_LIMIT` / `UPDATE_INSTRUCTION_SOFT_LIMIT` を事前 export する
  - 対象は `evm_canister`。`wrap_canister` の upgrade は [docs/ops/wrap-canister-deploy-runbook.ja.md](/Users/0xhude/Desktop/ICP/Kasane/docs/ops/wrap-canister-deploy-runbook.ja.md) の手順で別途実行する
- `scripts/mainnet/ic_mainnet_post_upgrade_smoke.sh`: デプロイ後の最小RPC確認
- `scripts/verify_submit_after_deploy.sh`: verify submit の手動/CIフック
- `scripts/mainnet/mainnet_method_test.sh`: 本番メソッド検証（重い）
- `scripts/mainnet/mainnet_wrap_unwrap_smoke.sh`: TESTICP を使った wrap -> unwrap 実経路確認
  - wrap 側は `quote_wrap_request` -> `submit_wrap_request` の新 API で実行する
  - unwrap の status 追跡は `get_unwrap_dispatch_overview` + `wrap_canister.get_request` を使う
  - unwrap 前に wrapped token の `approve(factory, amount)` を自動投入する
  - 破壊的 DID 変更後は script/client を canister と同時更新する前提
  - `MINING_IDLE_OBSERVE_SEC`: 冒頭の idle 観測秒数（既定: `6`）
  - `IDLE_MAX_CYCLE_DELTA`: idle 観測で許容する cycle 減少上限。`0` で閾値チェック無効（既定: `0`）

### prune運用
- `scripts/ops/apply_prune_policy.sh`: policy適用 + pruning有効化 + status確認
- `scripts/ops/tune_prune_max_ops.sh`: need_prune/error counters に基づく段階調整
- `scripts/ops/test_prune_ops_scripts.sh`: 上記2スクリプトのモック検証
- `scripts/ops/contabo_deploy_tools.sh`: Contabo上のgit作業ツリーから `tools/indexer` / `tools/explorer` を同期して build+restart（git ref指定運用）
- `scripts/ops/contabo_deploy_gateway.sh`: Contabo上のgit作業ツリーから `tools/rpc-gateway` を同期して build+restart（git ref指定運用）
  - Contabo で GitHub の HTTPS 認証を使わない場合は、`REPO_URL=git@github.com:<owner>/<repo>.git` を明示し、リモートの `deployer` ユーザーに read-only の SSH deploy key を持たせる
  - remote の clone/fetch は `deployer` 実行を前提にし、deploy key を `root` だけに置かない

## 主要環境変数（よく使うもの）
- `CANISTER_NAME` / `CANISTER_ID`
- `WRAP_CANISTER_ID`
  - 実際の `wrap_canister` principal が必要な wrap/unwrap smoke・ledger 系スクリプトで使用
- `EVM_WRAP_FACTORY`
  - `scripts/mainnet/ic_mainnet_deploy.sh` の必須値。20-byte EVM factory address を `0x...` 形式で渡す
- `QUERY_INSTRUCTION_SOFT_LIMIT`
  - 任意。設定すると `build_init_args_for_current_identity(...)` が `InitArgs.query_instruction_soft_limit` を出力する
- `UPDATE_INSTRUCTION_SOFT_LIMIT`
  - 任意。設定すると `build_init_args_for_current_identity(...)` が `InitArgs.update_instruction_soft_limit` を出力する
- `ICP_IDENTITY_NAME`
- `POCKET_IC_BIN`（`predeploy_smoke.sh` / `run_rpc_compat_e2e.sh` で使用するPocketICバイナリ）
  - 推奨: まず既存のローカルバイナリを指して、都度ダウンロードを避ける
- `E2E_TIMEOUT_SECONDS`（`run_rpc_compat_e2e.sh` のタイムアウト秒）
- `RUN_INDEXER_SMOKE`（`predeploy_smoke.sh` で local indexer smoke を追加実行。既定 `0`）
- `RUN_POST_SMOKE`（`ic_mainnet_deploy.sh` で post smoke を有効化）

## verifyを自動投入する（任意）

canister deployスクリプトとは独立で、必要なパイプラインから
`scripts/verify_submit_after_deploy.sh` を直接呼び出してください。

必要な環境変数:
- `VERIFY_PAYLOAD_FILE`（verify submit payload JSON のパス）
- `VERIFY_AUTH_KID`
- `VERIFY_AUTH_SECRET`
- 任意: `AUTO_VERIFY_SUBMIT`, `VERIFY_SUBMIT_URL`, `VERIFY_AUTH_SUB`, `VERIFY_AUTH_SCOPE`, `VERIFY_AUTH_TTL_SEC`

例:
```bash
AUTO_VERIFY_SUBMIT=1 \
VERIFY_PAYLOAD_FILE=/tmp/verify_payload.json \
VERIFY_AUTH_KID=kid1 \
VERIFY_AUTH_SECRET=replace_me \
scripts/verify_submit_after_deploy.sh
```

## 失敗時の切り分け
1. まず `scripts/query_smoke.sh` が通るか確認する  
2. 次に `scripts/run_rpc_compat_e2e.sh` を単体実行する  
3. 必要なら `scripts/local_indexer_smoke.sh` を実行する  

重いスクリプトで失敗したときは、単体スクリプトに分解して再実行すると原因特定が速いです。
