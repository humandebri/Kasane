# scripts/README.md

Shortest guide for operational scripts in this directory.
If unsure, run the commands in the order below.

## Prerequisites
- Working directory: repository root (`Kasane/`)
- Main dependencies: `cargo`, `icp`, `node`, `npm`, `python`
- For query calls, use `dfx canister call --query ...`
- Keep `dfx` only for query paths for now; use `icp` everywhere else
- For local verification, prefer PocketIC over ad-hoc local deploy flows
- If a local PocketIC binary already exists in the repo or workspace, prefer that binary over downloading one on demand

## Frequently Used Commands

1. CI-equivalent checks (light)
```bash
CI_LOCAL_MODE=github scripts/ci-local.sh
```

2. Pre-deploy smoke (standard, PocketIC)
```bash
scripts/predeploy_smoke.sh
```

3. Local integrated smoke (heavy)
```bash
scripts/local_indexer_smoke.sh
```

4. Query-only smoke
```bash
scripts/query_smoke.sh
```

5. Wasm dependency profiling (Phase 0.5)
```bash
scripts/profile_wasm_deps.sh --package ic-evm-gateway
```

6. Precompile ratio measurement
```bash
CANISTER_NAME_OR_ID=<id> \
WORKLOAD_CMD='scripts/playground_smoke.sh' \
scripts/measure_precompile_ratio.sh
```

## By Purpose

### Pre-checks and Quality Gates
- `bash scripts/verify-evm-proofs.sh`: joint Verus `--no-cheating` and Lean/revm gate for the EVM proof scope.
- `bash scripts/verify-lean.sh`: Lean 4.31.0 EVM adapter model proofs, axiom audit, `leanchecker` recheck, source drift check, and 439 Rust/Lean boundary cases. Requires `elan` with the pinned toolchain and `rustc`; see `proofs/evm/README.md`. Separate from the existing CI-equivalent command.
- `bash scripts/verify-revm.sh`: pinned revm source/features, Lean proofs, 402 result-size/asset-admission comparisons, rejected-result retry, journal trace comparison, official Prague state fixtures, and Kasane CALL/CREATE/SELFDESTRUCT/precompile authorization/rollback checks. CI runs this in the separate `evm-proofs` job. Requires Python 3 plus the Lean prerequisites; see `proofs/evm/revm-correspondence.md`.
- `scripts/ci-local.sh`: runs in `github|smoke|all` modes via `CI_LOCAL_MODE=<mode>`
- `scripts/ci_github_equivalent.sh`: single source of truth for the GitHub-equivalent checks used by both `.github/workflows/ci.yml` and `scripts/ci-local.sh`
  - includes Rust baseline quality gates: `rustfmt --check` for workspace Rust files except specgen-managed Verus contract targets, plus clippy with a `too_many_arguments` exception for specgen evidence functions
- `scripts/check_gateway_api_compat_baseline.sh`: detects breaking changes in gateway API compatibility baseline (`--update` updates baseline)
- `scripts/check_gateway_matrix_sync.sh`: verifies compatibility matrix row in `tools/rpc-gateway/README.md` matches `tools/rpc-gateway/package.json` version line
- `scripts/check_precompile_feature_isolation.sh`: verifies the default wasm build of `ic-evm-core` does not pull BLS/KZG backend crates (`ark-bls12-381`, `c-kzg`, `blst`)
- `scripts/check_icp_query_precompile_verification.sh`: PR #81 ICP query precompile gate; runs Verus, PBT/async/allowlist/PocketIC tests, Bidi control checks, workspace check, targeted rustfmt, and checks the PR-local specgen artifacts
  - Use this script as the PR #81 merge gate. `specgen gate --base origin/main` remains diagnostic for this PR because the current CLI requires targets for every changed Rust function, including async adapters, methods, and test helpers outside the five pure spec targets.
- `scripts/predeploy_smoke.sh`: `cargo check` + wasm build + PocketIC RPC compatibility E2E (optional indexer smoke)
- `scripts/run_rpc_compat_e2e.sh`: RPC compatibility E2E test (`cargo test --test rpc_compat_e2e`)
  - The script runs `forge build` first because the Rust E2E tests load Foundry artifacts from `tools/wrapper-vite/contracts/out/` at compile time
  - PocketIC must be able to bind to localhost (`127.0.0.1`); restricted sandbox environments can fail before the test logic runs
- `scripts/prepare_ci_icrc1_ledger_wasm.sh`: exports the vendored official ledger wasm at `third_party/dfinity/ledger-suite-icrc-2026-03-09/ic-icrc1-ledger.wasm` as `ICP_LEDGER_WASM` via the shared ledger artifact helper; `LEDGER_RELEASE=latest` is rejected and the local ledger smoke script caches `ledger.did` under `${LEDGER_CACHE_DIR}/<release>/ledger.did`
- `scripts/profile_wasm_deps.sh`: dependency-size profiling for wasm (`twiggy top/dominators`, optional `cargo +nightly bloat -Z build-std`, and `cargo tree -e features -i <crate>` snapshots)
  - output default: `docs/ops/reports/wasm-deps-<package>-<timestamp>/`
  - optional: `--compare <previous_output_dir>` to generate before/after table (bytes + instruction estimate)

### Local Operations
- `scripts/icp_local_clean_start.sh`: clean start helper for managed local network (`icp network`)
- `scripts/local_pruning_stage.sh`: staged pruning verification
- `scripts/local_indexer_fault_injection.sh`: indexer fault-injection test
- `scripts/measure_precompile_ratio.sh`: replays a workload, summarizes `get_precompile_profile`, and suggests a fixed precompile ratio
  - treat IC instruction counter as the source of truth; wall-clock timing is not used for charging decisions
  - verifies `clear_precompile_profile` before starting; if clear fails, the script stops instead of mixing stale profile entries into the measurement
  - set `TARGET_GAS_PER_INSTRUCTION` to use a fixed target directly
  - or set `REFERENCE_PRECOMPILE_ADDRESS` + `REFERENCE_TARGET_GAS` to derive a ratio from a measured reference precompile
  - example for heavy modexp calibration: `REFERENCE_PRECOMPILE_ADDRESS=0x0000000000000000000000000000000000000005 REFERENCE_TARGET_GAS=3000000`
  - the script does not mutate canister state; if you adopt a new ratio, update the fixed ratio in code and redeploy
- `scripts/run_precompile_profile_e2e.sh`: builds the latest gateway wasm, runs PocketIC, and prints measured `get_precompile_profile` entries for default targets (`ecrecover`, `blake2f`, `modexp`)
  - interpret the output using `total_instructions` / `avg_instructions`; PocketIC wall-clock timing is intentionally ignored
  - PocketIC must be able to bind to localhost (`127.0.0.1`); if sandbox execution fails with a bind error, rerun in a normal terminal environment
  - set `PRECOMPILE_PROFILE_TARGETS=ecrecover,blake2f,modexp,modexp_heavy` or another comma-separated subset to override targets
  - use `modexp_heavy` when you want a larger 32-byte modular exponentiation fixture instead of the default lightweight `modexp`
  - `p256` is available only when the current execution spec enables the RIP-7212 precompile
  - this script builds `ic-evm-gateway` with the `precompile-profile-admin` feature; the default canister build does not expose the measurement-only APIs
  - the postprocessed wasm uses `crates/ic-evm-gateway/evm_canister_precompile_profile_admin.did` for candid metadata and endpoint validation
  - cleanup memo: `docs/ops/precompile_profile_cleanup.md`
  - sample saved output (`PRECOMPILE_PROFILE_JSON_PATH=/tmp/precompile_profile.json`):
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

### Playground
- `scripts/playground_manual_deploy.sh`: manual deployment to playground
- `scripts/playground_smoke.sh`: end-to-end Tx/RPC checks on playground
  - set `FUNDED_ETH_PRIVKEY` for additional transfer checks

### Mainnet Operations
- `scripts/mainnet/ic_mainnet_preflight.sh`: minimum pre-mainnet checks
  - `CANISTER_NAME` defaults to `evm_canister`
  - default cycles floor is `MIN_CYCLES=2000000000000`
  - when a legacy standalone wrap canister exists, set `LEGACY_WRAP_CANISTER_ID` and `LEGACY_WRAP_REQUEST_IDS_FILE` to prove all requests are drained
  - when there are no legacy request ids, `ALLOW_EMPTY_LEGACY_WRAP_REQUESTS=1` must be explicit
- `scripts/mainnet/ic_mainnet_deploy.sh`: main deployment script
  - the default build does not enable the `precompile-profile-admin` feature
  - measure precompile ratio before deploy with `scripts/run_precompile_profile_e2e.sh` / `scripts/measure_precompile_ratio.sh`; if you need to change the default fixed ratio `1/100`, rebuild and redeploy
  - even with `MODE=upgrade`, `WRAP_CANISTER_ID` and `EVM_WRAP_FACTORY` are required
  - set `QUERY_INSTRUCTION_SOFT_LIMIT` / `UPDATE_INSTRUCTION_SOFT_LIMIT` only when you intentionally want install / upgrade to overwrite the current soft limits
  - when deploying wrapper-vite at the same time, deploy `evm_canister` -> frontend; the frontend depends on `get_unwrap_request_ids_by_eth_tx_hash`
- `scripts/mainnet/ic_mainnet_post_upgrade_smoke.sh`: minimum RPC checks after deploy
- `scripts/verify_submit_after_deploy.sh`: manual/CI hook for verify submit
- `scripts/mainnet/mainnet_method_test.sh`: heavy mainnet method test
  - `MINING_IDLE_OBSERVE_SEC`: idle observation seconds at start (default: `6`)
  - `IDLE_MAX_CYCLE_DELTA`: allowed cycle decrease in idle observation. `0` disables threshold check (default: `0`)
- `scripts/report_icrc1_logos.sh`: collect `icrc1:logo` from `icrc1_metadata` and save a markdown report under `docs/ops/reports/`

### Prune Operations
- `scripts/ops/apply_prune_policy.sh`: apply policy + enable pruning + status check
- `scripts/ops/tune_prune_max_ops.sh`: staged tuning based on need_prune/error counters
- `scripts/ops/test_prune_ops_scripts.sh`: mock tests for the two scripts above

## Key Environment Variables
- `CANISTER_NAME` / `CANISTER_ID`
- `WRAP_CANISTER_ID`
  - integrated wrap uses the `evm_canister` principal
- `LEGACY_WRAP_CANISTER_ID`
  - legacy standalone wrap canister for preflight drain checks; the gate runs only when it differs from `EVM_CANISTER_ID`
- `LEGACY_WRAP_REQUEST_IDS_FILE`
  - legacy wrap request id manifest. Empty files require `ALLOW_EMPTY_LEGACY_WRAP_REQUESTS=1`
- `EVM_WRAP_FACTORY`
  - required by `scripts/mainnet/ic_mainnet_deploy.sh`; pass the 20-byte EVM factory address as `0x...`
- `QUERY_INSTRUCTION_SOFT_LIMIT`
  - optional; when set, `build_init_args_for_current_identity(...)` emits `InitArgs.query_instruction_soft_limit`
- `UPDATE_INSTRUCTION_SOFT_LIMIT`
  - optional; when set, `build_init_args_for_current_identity(...)` emits `InitArgs.update_instruction_soft_limit`
- `ICP_IDENTITY_NAME`
- `POCKET_IC_BIN` (PocketIC binary used by `predeploy_smoke.sh` / `run_rpc_compat_e2e.sh`)
  - Recommended: point this to an existing local binary first to reduce flaky downloads
- `E2E_TIMEOUT_SECONDS` (timeout for `run_rpc_compat_e2e.sh`)
- `RUN_INDEXER_SMOKE` (enable local indexer smoke in `predeploy_smoke.sh`; default `0`)
- `RUN_POST_SMOKE` (enable post-smoke in `ic_mainnet_deploy.sh`)

## Auto-submit verify (optional)

Independent of canister deployment scripts, call `scripts/verify_submit_after_deploy.sh` directly from required pipelines.

Required environment variables:
- `VERIFY_PAYLOAD_FILE` (path to verify-submit payload JSON)
- `VERIFY_AUTH_KID`
- `VERIFY_AUTH_SECRET`
- Optional: `AUTO_VERIFY_SUBMIT`, `VERIFY_SUBMIT_URL`, `VERIFY_AUTH_SUB`, `VERIFY_AUTH_SCOPE`, `VERIFY_AUTH_TTL_SEC`

Example:
```bash
AUTO_VERIFY_SUBMIT=1 \
VERIFY_PAYLOAD_FILE=/tmp/verify_payload.json \
VERIFY_AUTH_KID=kid1 \
VERIFY_AUTH_SECRET=replace_me \
scripts/verify_submit_after_deploy.sh
```

## Failure Triage
1. First, confirm `scripts/query_smoke.sh` passes
2. Then run `scripts/run_rpc_compat_e2e.sh` alone
3. If needed, run `scripts/local_indexer_smoke.sh`

If a heavy script fails, break verification into standalone scripts for faster root-cause isolation.

## RustからLeanへの実装対応

`bash scripts/install-aeneas.sh` で固定された証明用ツールを導入し、
`bash scripts/verify-rust-lean.sh` で本番Rustの再抽出一致、本番13関数とruint桁演算2関数の全入力対応を検査する。
依存・信頼境界は `proofs/extraction/README.md`、外部挙動の範囲は `proofs/external/README.md`。

固定revmのgas計上2関数の実抽出・全入力仕様（実験版）:
`bash scripts/verify-revm-gas-experimental.sh`。
本番root Cargo.lockとgas.rs・抽出器バイナリをハッシュ固定し、Lean再生成一致と
警告検査・公理監査・leancheckerを実行する。全EVM命令の実装対応は含まない。

固定revmの命令結果分類4関数（全32種類、実験版）:
`bash scripts/verify-revm-result-experimental.sh`。
本番root依存profileで実ソースの再生成一致、分類の完全性・排他性のLean証明、
警告検査・公理監査・leancheckerを実行する。命令実行やhalt後の復元は含まない。

固定ledger時刻ソースの実験的な再抽出・全入力証明の検査:
`bash scripts/verify-ledger-time-experimental.sh`。
構築・読み取り・checked算術に限り、Duration加減算と本番ledger/Wasm全体は未証明。

U256の実AND/OR/XORの全入力・全bit対応の実験ゲート: `bash scripts/verify-bitwise-experimental.sh`。
EVMのstack/gas/例外処理への接続、NOT、通常Rust/Wasmとの同値性は未証明。

実U256 AND/OR/XOR trait演算子の全入力ループ・全bit対応の実験ゲート: `bash scripts/verify-operators-experimental.sh`。
EVM命令のstack/gas/例外処理との対応は未証明。Step追加fieldは分離した実験バックエンドを使う。

実命令LLBCの未使用traitメタデータ処理の回帰検査: `bash scripts/verify-array-default-regression.sh`。
前処理の通過と参照される不正implの拒否のみを検査する。EVM命令の対応証明ではない。

本番root依存で実Stack len/is_empty/peekの全入力対応を検査: `bash scripts/verify-revm-stack-experimental.sh`。
更新系スタック操作、命令側のstack/gas/例外処理と本番Wasm対応は未証明。

### 未採用の参照コピー融合候補

`bash scripts/verify-ref-copy-candidate.sh`は固定された実ADD/SUBの中間コードで、
隣接する参照コピーの融合対象と拒否条件を検査する。6個の正例と12個の負例を確認し、
終了時にツールソースを復元して既存の固定バイナリhashを確認する。
これは候補の回帰検査であり、意味保存、命令対応、GAT警告の解消は未証明。
既存のOCaml/opamと固定Aeneas/Charonソースが必要。

`bash scripts/verify-ref-copy-fragment.sh`は同候補の局所断片モデルを検証する。
4定理、18宣言の公理監査、カーネル再検査、負例3件の拒否を確認する。
Rust/LLBCとの意味論の対応は含まれず、候補を適用・採用しない。

`bash scripts/diagnose-revm-gat.sh`はMemoryTrのGAT/RPITIT抽出制約を
依存のないRustコード3例で再現する。Rustで有効な3例のうち通常の参照戻り型のみ
Charon・Aeneasで抽出でき、残り2例を固定Charonが拒否することを確認する。
既知の失敗を再現する診断であり、全命令の証明ゲートには含めない。

`bash scripts/verify-gat-closed-candidate.sh`は隔離された未採用Charon候補を
検査する。閉じたGAT等式の4例の成功、型引数に依存する例の拒否、
`[u8]`/`[u16]`の区別と生存期間binder、実ADD/SUBの固定fixtureのエラー不在を
確認する。全命令の証明ゲートには含めない。候補の正当性とLean側のGAT対応は未完了。

`bash scripts/verify-typeinfo-experimental.sh`は固定Charonの実型判定を再生成し、
全入力対応8定理、31宣言の監査、カーネル再検査、負例3件の拒否を確認する。
公開ライブラリのno-default-features構成を使う。AST flag計算やGAT変換全体の
正当性、native feature構成の同値性は含めない。

`bash scripts/verify-revm-add-sub-borrow-candidate.sh`は隔離された未採用候補で、
固定した実ADD/SUBとラッパーの4本体を記号的に借用検査する。65個のOpaque関数宣言の
内部は検査しない。固定Aeneasのmutable Copy拒否と、候補Lean変換のMemoryTr GAT拒否も
対照検査する。保存fixtureの診断であり、全入力の命令同値性の証明ではない。
既存候補バイナリとその固定hashを必要とし、通常ツールは変更しない。

`bash scripts/diagnose-revm-instructions.sh`は固定revm全命令テーブルの再抽出診断。
既知150opcode・未知106byteを照合し、151ラッパーと全85種類の実命令本体を
毎回Rust検査・Charon再抽出する。236本体のエラー型不在と、候補Aeneasが
MemoryTrの具体的Ref/GAT戻り型不一致を厳格に拒否することを確認する。
273個のOpaque宣言、別probe graph、static gas/host/unsafe実装の境界があり、
全命令の借用・Lean証明の成功を表すゲートではない。隔離した両候補が必要。

`bash scripts/verify-rpit-signature-candidate.sh`は未採用の宣言RPITIT戻り型候補を検査する。
既定の借用スライスとoverrideのVecを混同せず、宣言のGAT射影を保持することを
Rust実行と旧/新候補の型抽出で確認する。既存GAT例4個の成功・依存型GATの拒否と、
`diagnose-revm-instructions.sh rpit-signature`による全命令の再抽出も含む。
従来の戻り型不一致は越えるが、Deref/GAT借用比較は未対応であり、命令のLean証明はない。
通常の`diagnose-revm-instructions.sh`は従来候補の診断をそのまま再現する。

`bash scripts/verify-gat-borrow-obligations.sh`は固定analyze_tyで実借用スライスと
対応GATのborrow flag差を直接検査し、専用診断で異なる生存期間・同じ消去型・
GATの借用flag falseと元の厳格な拒否を確認する。借用位置の条件付きモデル
11定理を警告検査・公理監査・カーネル再検査し、負例3件も拒否する。
GAT比較器を緩めたり採用したりするゲートではなく、Rust/LLBC/OCaml対応は未証明。
隔離した診断runnerと固定型解析probe、Charonの戻り型候補を必要とする。

`bash scripts/verify-ledger-duration-conditional.sh`は固定公式TimeStamp Add/Subの
変更なしの分割抽出を再生成し、明示的Duration/Debug/unwrap外部モデル下の15定理を検証する。
実Rust境界テスト2件、59定理の公理監査、警告・独立カーネル検査、不正公理/sorry/native
負例3件、および完全Duration抽出のTPattern未対応を確認する。
実Rustの外部モデルとのrefinementやIC/ledger/Wasm全体の同値性は未証明。

`bash scripts/verify-ledger-duration-source.sh`は実std Durationの型・2演算・2定数と
TimeStampの加減算等を変更なしで抽出する。外部モデルは範囲付きNanosecondsの
生成/読取り、Debug/unwrap観測、エラーpayload aliasとして明示する。
19条件付き定理と旧モデルへの5対応定理、86宣言の公理監査・警告/カーネル検査、
実Rust境界テスト3件、再抽出一致、負例3件と完全範囲型抽出の拒否を検査する。
unsafe transmuteと実Rust/native/Wasm/全EVM/IC・ledger外部挙動の対応は未証明。

`bash scripts/build-u32-range-analysis-candidate.sh`は未採用のU32定数範囲借用解析候補を
隔離exeへビルドし、固定TypesAnalysisソースを終了時に復元する。固定6パッチの
バイナリを更新しない。生成exeは`.local/proof-tools/aeneas-u32-range-analysis`。

`bash scripts/verify-u32-range-analysis-candidate.sh`は範囲付き完全型/opaque型の両経路で
実Nanosecondsのunsafe本体を毎回エラーなしに抽出する。型・範囲・本体の保持、
20組の借用/outlive分類比較と12件の不正/未対応範囲の拒否をnativeで検査する。
固定版の範囲型解析停止、候補のPure型変換停止、両版のtransmute拒否も厳格に検査する。
候補の型解析意味保存とunsafe変換は未証明で、Lean出力も取り込まない。
固定範囲・hashは`proofs/extraction/tool-patches/u32-range-analysis-profile.json`。

`bash scripts/build-u32-range-pure-candidate.sh`は未採用のU32定数範囲をLean Subtypeへ
抽出する候補を隔離ビルドする。7ソースをhash固定し、終了時に復元する。
出力は`.local/proof-tools/aeneas-u32-range-pure`で、固定版exeを更新しない。
`bash scripts/verify-u32-range-pure-candidate.sh`は実Nanosecondsの完全範囲と
Duration.as_secsをクリーン再生成し、生成ファイル一致、5定理・11宣言の監査、
カーネル再検証と公理/sorry/native_decideの負例を検査する。20組の分類比較・
12範囲負例と、実unsafe両方向のtransmute拒否を維持する。翻訳器の意味保存と
unsafe変換のRust/Lean対応は未証明。固定範囲は
`proofs/extraction/tool-patches/u32-range-pure-profile.json`。

`bash scripts/build-nanoseconds-read-candidate.sh`は実Nanoseconds読取り方向だけの
nativeレイアウト検査付き候補を隔離ビルドし、固定12ソースを終了時に復元する。
出力は`.local/proof-tools/aeneas-nanoseconds-read`。固定版のexeを更新しない。
`bash scripts/verify-nanoseconds-read-candidate.sh`は型・実読取りとDurationのgetterを
クリーン再生成し、5実本体定理・6モデル対応定理/24宣言の監査・カーネル検査・負例を確認する。
不正レイアウトとunsafe生成は拒否する。実std境界テストをRust/Miriでも確認する。
翻訳器・レイアウト意味保存とWasm対応は未証明。固定範囲とhashは
`proofs/extraction/tool-patches/nanoseconds-read-profile.json`。

`bash scripts/build-nanoseconds-construct-candidate.sh`は範囲内のunsafe生成を部分的に
解釈する候補を隔離ビルドし、固定12ソースを復元する。Mainもhash固定する。
出力は`.local/proof-tools/aeneas-nanoseconds-construct`。固定版exeを変更しない。
`bash scripts/verify-nanoseconds-construct-candidate.sh`は実Durationの生成/読取りと
両Nano castをクリーン再生成し、12部分的/生成コード定理・9モデル対応定理と
48宣言の公理/カーネル監査を検証する。全u64入力でのunsafe前提成立を含む。
範囲外のundefは候補モデルの表現で、Rustの実行結果や全UB挙動の主張ではない。
壊れたレイアウト・opaque型・固定版/旧候補の失敗を負例として維持する。
Rust/Miri境界テストも実行する。翻訳器/レイアウト意味保存とWasm/全EVM/IC外部
対応は未証明。固定範囲は`proofs/extraction/tool-patches/nanoseconds-construct-profile.json`。

`bash scripts/verify-ledger-duration-full.sh`は完全な範囲付きNanosecondsと実両castを
ledger TimeStamp加減算まで接続した未採用候補の検証。実本体10個・initializer2個を
エラーなしで抽出し、生成ファイルのbyte一致、19条件付き定理・5モデル対応定理、
86宣言の公理/警告/カーネル監査と負例を確認する。Rust境界テストとMiriも実行する。
Nanoの手書き外部型/生成/読取りを使わない。Debug/unwrap・エラーaliasは外部モデルで、
候補の意味保存や実Rust/native/Wasm/全EVM/IC外部対応は未証明。
固定範囲は`proofs/extraction/ledger-duration-full-profile.json`。

`bash scripts/build-int-error-source-candidate.sh`は実TryFromIntErrorをUnitにせず保持する
候補を`.local/proof-tools/aeneas-int-error-source`へ隔離ビルドする。13変更ソースを
終了時に復元し、Mainもhash固定する。自動生成BuiltinLean登録ファイルは変更しない。
`bash scripts/verify-ledger-duration-typed-error.sh`は手書きNano/error aliasを除いた
ledger加減算をクリーン再生成し、19条件付き定理・5対応定理と86宣言の監査を検査する。
実/不正エラー型と他のLean builtin登録不変、Nano guard、opaque/変更LLBC型の拒否、
Rust/Miriでの実エラーペイロード境界テストも確認する。手書き外部定義はDebug/unwrap。
翻訳/型簡約・runtime/Wasm/全EVM/IC対応は未証明。固定範囲は
`proofs/extraction/ledger-duration-typed-error-profile.json`と
`proofs/extraction/tool-patches/int-error-source-profile.json`。

`bash scripts/build-never-call-candidate.sh`は実unwrapとNever正常復帰側の空型消去を
`.local/proof-tools/aeneas-never-call`へ隔離ビルドする。16変更ソースを復元し、Mainも
hash固定する。既存固定バイナリを保持し、他のLean builtin関数登録は変更しない。
`bash scripts/verify-ledger-actual-unwrap.sh`は13本体のクリーン抽出と無編集byte一致、
19条件付きDuration・5対応・6Never/unwrap定理、115宣言の公理監査と独立カーネルを
確認する。旧候補のNever fallthrough失敗、変更型/レイアウトの拒否、axiom/sorry/native
負例、16文字列バイト出力ケース、実Rust unwrapの成功/panic観測とMiriも含む。
手書き外部観測はDebug/unwrap_failedで、formatting・unwind/destructor・翻訳器意味保存・
native/Wasm/全EVM/IC外部対応は未証明。LLBCにon_unwindが残るだけでは証明としない。
固定範囲は`proofs/extraction/ledger-actual-unwrap-profile.json`と
`proofs/extraction/tool-patches/never-call-profile.json`。

### Ledger panic payload (experimental)

`bash scripts/build-dyn-debug-candidate.sh`で動的Debug/引数保持の隔離候補を構築する。
`bash scripts/verify-ledger-panic-payload.sh`は固定hash、Rust/Miri、14本体再抽出、
生成ファイル一致、通常preset対照、52設定負例、Nativeのshape負例、
40条件付き定理/153宣言公理監査、独立kernel、axiom/sorry/native負例を検査する。
外部fmt/panicは明示モデル。実外部runtime・IC/ledger同値性の完了を意味しない。
profileは`proofs/extraction/ledger-panic-payload-profile.json`。

### Actual ledger formatting source diagnostic

`bash scripts/build-formatting-source-candidate.sh`でformatter builtinを外した隔離候補を構築する。
`bash scripts/diagnose-ledger-formatting-source.sh`は17実本体、受け手/関数ポインタの元の操作、
実フィールド、29 FnPtr型、全他builtin登録の維持を確認し、型翻訳の厳格失敗と5設定負例を再現する。
期待結果はLean出力前のunsupported-arrow拒否。対応証明の成功を意味しない。
固定範囲は`proofs/extraction/tool-patches/formatting-source-profile.json`。

### Closed source function pointer signature diagnostic

`bash scripts/build-closed-fnptr-candidate.sh`で閉じたFnPtr署名を保持する隔離候補を構築する。
`bash scripts/diagnose-ledger-closed-fnptr.sh`は17本体再抽出、29出現中13閉signatureの
metadata保持/16 open出現の拒否、12不正signature/3 backend拒否、unsafeの区別と型代入不変を検査する。
実本体はfn itemのlate-bound regionと配列参照→NonNull transmuteで厳格停止する。
成功した対応証明とは数えず、Lean出力0と旧候補の型障害を確認する。
固定範囲は`proofs/extraction/tool-patches/closed-fnptr-profile.json`。

### Function binder region erasure diagnostic

`bash scripts/build-fn-region-candidate.sh`はscope/binder保持の隔離候補を構築し、
Charon自身のプロジェクトでML testsを明示ビルドして強制実行する。
`bash scripts/diagnose-ledger-fn-region.sh`は17本体再抽出、実33 function signature、
36 depth/id組、元の拒否/自由region消去、前候補比較、5入力負例を検査する。
trait関数項目の既存binder代入による署名解決も含み、実2 reificationの変換先との完全一致、
欠落trait/methodと高階trait binderの拒否、安全性/ABI/variadicの区別をNative検査する。
6 integer-depthモデル補題の公理/カーネルとaxiom/sorry/native負例も検査する。
期待する実本体の停止はreification/ref-pointer castで、Lean対応証明の成功ではない。
固定範囲は`proofs/extraction/tool-patches/fn-region-profile.json`。

### Exact function-item symbolic cast diagnostic

`bash scripts/build-fn-reify-candidate.sh`はexact signature castとfunction-only region
inventory/値型消去/ETY scopeの隔離候補を構築し、Charon standalone ML testsも実行する。
`bash scripts/diagnose-ledger-fn-reify.sh`は17本体を再抽出し、実2 reification、14 Native target負例、
実cast ABI/safety/variadicの3負例、33 function region/ETY検査、前候補のcast停止、5設定負例を検査する。
期待する次の停止はFnPtr greedy borrow expansion(InterpExpansion731)とpointer transmute。
Lean出力や新しいRust対応定理はなく、既存6モデル補題の検査をreification証明として数えない。
固定範囲: `proofs/extraction/tool-patches/fn-reify-profile.json`。

### Function-pointer stored-borrow diagnostic

`bash scripts/build-fn-value-candidate.sh`はsignature borrowと値のstored borrowを分ける
TypesAnalysisの隔離候補を構築し、Charon standalone ML testsも明示ビルド/強制実行する。
`bash scripts/diagnose-ledger-fn-value.sh`は実29 FnPtr/58外側・隣接contextとnested context、
固定Rust/Miri各3観測、capture/returned lifetime/outer mutable borrowの3compile-fail、
17本体の新規抽出、前候補のgreedy expansion停止、既存signature/region/設定負例を検査する。
期待する次の停止はsafe→unsafe FnPtr transmuteとref→NonNull transmute。Lean出力/新Rust定理は0。
固定範囲: `proofs/extraction/tool-patches/fn-value-profile.json`。

### Function-pointer representation transmute diagnostic

`bash scripts/build-fn-transmute-candidate.sh`は同じRust ABIのFnPtr表現変換をsymbolic演算として
保持する隔離候補とfunction-only region predicateを構築し、Charon standalone ML testsを実行する。
`bash scripts/diagnose-ledger-fn-transmute.sh`は17本体再抽出、実2 transmute/22 Native対照/
3実target変異/33 region predicate、private receiver pairのRust/Miri各5観測、Miri専用wrong receiver
拒否例、前候補と設定対照を検査する。wrong receiverはnatively実行せず、Miri targetも分離する。
期待する次の停止はhigher-ranked TFnDefのPure型表現とref→NonNull。call ABI/receiverの証明ではない。
固定範囲: `proofs/extraction/tool-patches/fn-transmute-profile.json`。

- `build-fn-item-candidate.sh` / `diagnose-ledger-fn-item.sh`: 実4 trait function-item型の
  metadataと代入可能なSelf/trait証拠を保持する実験候補。20付替え・60拒否対照、前候補停止を確認。
  callable抽出は未対応で、実17本体はfunction castの値変換で停止する。Lean出力/新Rust定理は0。

- `build-fn-cast-candidate.sh` / `diagnose-ledger-fn-cast.sh`: Pure function-item coercionの
  両端の型を代入可能な形で保持する実験候補。署名/operand照合とextractor拒否を維持する。
  実17本体は自由Self型を含むキャスト先FnPtr変換で停止し、実Pure cast値/Lean出力は0。

- `build-fn-template-candidate.sh` / `diagnose-ledger-fn-template.sh`: 自由型変数を持つ
  FnPtrを明示的型スロットとPure型引数で保持する実験候補。実16型の再構成/代入、64付替え、
  192拒否対照を確認。実17本体の次の停止はPure transmuteで、Lean出力/新Rust定理は0。

- `build-fn-pure-transmute-candidate.sh` / `diagnose-ledger-fn-pure-transmute.sh`: typed Pure
  FnPtr transmuteの実験候補。実2演算の型/代入と22署名変形・他backendの早期拒否を確認。
  実17本体はtrait function-item値変換で停止し、Lean出力/新Rust定理は0。unsafe callは未証明。

- `build-fn-item-value-candidate.sh` / `diagnose-ledger-fn-item-value.sh`: trait関数値の
  元型binder/消去済み値寿命/identity/captureを保持する実験候補。実PrePassesで2定数を検証し、
  12不正値/18型対照等を拒否する。実17本体は16/17、残る停止は参照→NonNull transmute。
  Lean出力/新Rust定理は0。実call/external semanticsは未証明。

- `diagnose-ledger-nonnull-layout.sh`: 基準17本体とNonNullだけ追加includeした補助抽出を比較し、
  実transparent/NotNull fieldと記号layout保証を確認する。12 source-schema対照、Rust/Miri5観測、
  ゼロ長arrayのMiri専用read負例を検証。Native/Pure変換・全入力Rust対応定理は未追加。

- `build-notnull-pattern-candidate.sh` / `diagnose-ledger-notnull-pattern.sh`: 実NonNull定義の
  NotNull pointer型/capture/mutability、格納borrow flagsとpointee region比較を保持する候補。
  6型対照・他backend・7 Native/5 CLI設定対照を拒否。追加定義込み16/17、残る停止は
  参照→NonNull変換。pointer value semantics/Lean出力/新Rust対応定理は未追加。

- `build-array-nonnull-candidate.sh` / `diagnose-ledger-array-nonnull.sh`: 固定ターゲットの
  array参照→NonNull表現照合を行う隔離候補。既存body-region消去後の実2変換、46型/宣言/layout負対照、
  Rust/Miri各5観測を確認する。新鮮な17本体は16/17でpointer region/provenance義務を明示して停止し、
  Lean出力0を検証する。元borrowの寿命・pointer値意味論・全入力対応定理は未証明。

- `build-array-nonnull-regions-candidate.sh` / `diagnose-ledger-array-nonnull-regions.sh`: 実共有borrowから
  Symbolic array loanを検索し、pointeeの完全な寿命付き型を保持する候補。
  型消去との一致・free/ended region判定・13不適合loan対照とRust/Miri各5観測を検証する。
  新鮮な実Arguments.newはSymbolic評価後にPure pointer value extraction義務で16/17停止、Lean出力0。
  実メモリ・provenance・Rust元lifetime・全入力対応の証明は未追加。

- `build-array-nonnull-pure-candidate.sh` / `diagnose-ledger-array-nonnull-pure.sh`: Pure array→NonNull演算に
  interpreterの実borrow/shared/loan由来IDと型代入可能な端点を保持する候補。
  21不正origin/loan/destination対照と3他backend、後処理済み実17本体とArguments.newの2演算を直接検査する。
  新鮮なCLIはPure/後処理17/17後、NonNull型抽出を明示的に拒否。不完全Types.lean1つを隔離し、
  完成Lean/oleanは0。Rust/Miri各7観測が通るが実address/provenance/全入力対応は未証明。

- `diagnose-shared-array-pointer-footprint.sh`: 非nullとは別の範囲・temporal/read権限とalias解決義務を
  14 MODEL定理としてwarningAsError、公理監査、leancheckerで検証する。
  axiom/sorry/native_decide混入と非nullだけで正サイズreadを正当化する誤証明を拒否。
  実Rust pointer/refinementやread安全性は未証明で、Aeneasのpointer抽出拒否は変更しない。

- `reproduce_icrc_ledger_wasm.sh`: 固定公式ledger source/image digest/ビルド入力を照合し、
  Apple containerの専用linux/amd64環境でlocal/stamped Wasmを再ビルドする。
  同名既存containerには触れず、全バイト比較を行う。初回再現ビルドは同梱Wasmと一致した。
- `audit_icrc_ledger_artifact.py` / `test_icrc_ledger_artifact_metadata.py`: ソース・固定artifact hashと
  埋め込みcommit/Candidを監査し、7metadata負対照と変更artifactの拒否を検証する。
  再ビルド比較は`--rebuilt-wasm <path>`。Wasm命令/IC意味論の証明ではない。

- `audit_release_duration_mir.py`: 固定release/wasm32とnightly/hostの4 Duration probe MIRを照合する。
  as_nanos本体の完全一致、係数/変換上限/comparison/saturationの構造と4改変拒否を検証する。
  error payloadとunwrap CFGは異なる。テキスト照合でありMIR意味論・全入力対応の証明ではない。

- `replay_ledger_core_mir.py`: 固定公式Bazelのledger_core Rustcアクションを、出力指定だけ変更してMIRへ再実行する。公式ソース・依存が揃ったLinux execution_root内で使用する。
- `audit_ledger_core_release_mir.py`: 取得済み実TimeStamp MIR・コンパイル引数・固定SHAを監査する。8件の改変拒否を含む。意味論の証明ではない。

- `extract_release_timestamp_sequence.py [--check]`: 固定公式TimeStampの実MIR証跡を監査し、実ローカル番号を限定Lean呼び出し列へ写す。フロントエンドの意味論対応は未証明。
- `verify-release-timestamp-sequence.sh`: 生成列の11件の条件付き補題・全入力定理を固定SHA、厳格Lean、公理依存監査、独立カーネル、4件の否定対照で検証する。標準ライブラリ契約やIC全挙動を無条件で証明するものではない。

- `extract_release_timestamp_cfg.py [--check]`: 固定実TimeStamp MIRの各基本ブロック・戻り先を限定Lean CFGへ写す。未知の文/終端/戻り先など5否定対照を含む。Storageや参照の抽象化は未証明。
- `verify-release-timestamp-cfg.sh`: CFG→呼び出し列の一般的意味論保存と実加減算CFGの11定理を、固定SHA・厳格Lean・公理監査・独立カーネル・6否定対照で検証する。

- `extract_release_timestamp_lifetime.py [--check]`: 実MIRのStorage/copy/borrow/move/call/returnを20イベントずつ取得し、寿命イベント改変3件が生成物に反映されることを確認する。
- `verify-release-timestamp-lifetime.sh`: ローカル寿命/初期化モデルの16定理、両Move方針、全接頭辞、独立カーネル、6否定対照を検証する。Rust参照やprovenanceの意味論は未証明。

- `verify-release-timestamp-lifetime-soundness.sh`: 寿命checker成功が各読み出しのlive/初期化と各値書込み先のliveを保証する一般健全性・実列への適用6定理を、固定SHA・厳格Lean・公理監査・独立カーネル・5否定対照で検証する。

- `diagnose-revm-jumpdest.sh`: 実JUMPDESTを本番rootの汎用・部分単相化経路と既存wrapperの完全単相化経路で新規抽出し、GAT/消去寿命の厳格拒否・Lean出力なしを再確認する診断。命令証明ゲートではない。
- `audit_jumpdest_llbc.py`: 取得済み実JUMPDEST LLBCのunit-only文形と可変参照/binderを監査し、本体・参照・GATの寿命/制約/default/boundを監査し、3経路合計のコピー証跡改変19件を拒否する。Rust/LLBC意味論の証明ではない。

- `verify-mono-lifetime-fixture.sh`: 依存なし実Rust寿命例の汎用抽出と8定理を固定SHA・生成byte一致・厳格Lean・公理監査・leancheckerで検証する。完全単相化の寿命消去による厳格拒否も再現し、4 LLBC改変と5偽証明を拒否する。EVM/メモリ同値性の証明ではない。
- `audit_mono_lifetime_llbc.py`: 取得済み寿命例LLBCのSame/Split宣言binder、左右の参照・返す参照の寿命関係を監査する。寿命の発明・統合・消失・返り先の取り違え4件を拒否する。

- `verify-revm-jumpdest-experimental.sh`: 実Interpreter/Gasも透明に抽出した実JUMPDEST生成本体の全入力2定理を、新規抽出byte一致・厳格Lean・公理監査・leanchecker・17件の否定対照で検証する。未使用trait節除去の意味論保存とRust/EVM実行対応は未証明。
- `audit_jumpdest_transparent_llbc.py`: 実命令のunit-only本体、Context寿命/型引数/outlives、保持GAT、実Interpreter/Gasフィールドを監査する。コピーしたLLBCの本体・型・制約など12改変を拒否する。

- `diagnose-revm-step-dispatch.sh`: 実step/static_gas/record_cost_unsafe/halt_oog/execute/haltの6本体を本番rootから抽出し、関数ポインタ型での厳格拒否・Lean成果物なしと、基準版/既存3候補のborrow-only経路の拒否箇所を再現する診断。ディスパッチ証明ゲートではない。
- `audit_revm_step_llbc.py`: 実LLBCの呼出し順、PC増分1、OOG分岐、self.fn_と元のctxの由来、間接呼出し、関数ポインタABI/寿命/型束縛を監査する。コピーした証跡の改変9件を拒否する。実行意味論の対応は未証明。

- `build-dynamic-borrow-candidate.sh`: 既存fn-value列とborrow-only間接呼出し処理の分離候補をビルドし、強制Charon MLテスト後に基準ソースを復元する。未採用で、既存バイナリは置き換えない。
- `verify-dynamic-borrow-candidate.sh`: 新規実step抽出の6透明本体を候補で借用検査し、LLBC改変9件・ネイティブ署名6件を拒否する。Lean生成は厳格拒否し、実行対応証明とは扱わない。

- `build-dynamic-decomposition-candidate.sh`: 宣言IDを要求しないeffect付き署名分解の分離候補をビルドし、強制MLテスト後に基準ソースを復元する。未採用。
- `verify-dynamic-decomposition-candidate.sh`: 実callbackのContext/backward状態/Result/effect型保持と通常6署名のAPI配線一致、再抽出・借用検査・15否定対照・Lean厳格拒否を検証する。実行意味論の証明ではない。

- `build-dynamic-symbolic-candidate.sh`: 評価済み関数値を保持する動的呼出しASTの分離候補をビルドし、強制MLテスト後に基準ソースを復元する。未採用。
- `verify-dynamic-symbolic-candidate.sh`: 実executeを記号実行してcallback/Context/署名/借用continuationのAST保持を確認し、再抽出・6本体借用検査・15否定対照・Lean厳格拒否を検証する。全実行対応の証明ではない。

- `build-dynamic-application-candidate.sh`: 元のcallback値へPure applicationを生成する候補を、厳格な入力/Result/backward型一致検査付きで分離ビルドする。全FnPtr型翻訳は未対応。
- `verify-dynamic-application-candidate.sh`: 実署名に基づく抽象Pure変数application・3型拒否、実AST保持・6本体借用検査・再抽出・18否定対照を検証する。全体Lean生成は先行するopen FnPtrで厳格拒否される。

- `build-dynamic-fnptr-candidate.sh`: Contextを参照する実callback型翻訳の分離候補をビルドし、強制MLテスト後に基準ソースを復元する。未採用。
- `verify-dynamic-fnptr-candidate.sh`: 実FnPtr/Instructionフィールド型、application型との一致、実AST/6本体借用検査・24否定対照を検証する。全体Lean翻訳は次のMemoryTr GATで厳格拒否され、成果物なし。

- `build-dynamic-effects-candidate.sh`: 未知の間接呼出しの失敗/発散を関数効果解析へ反映する分離候補をビルドし、強制MLテスト後に基準ソースを復元する。
- `verify-dynamic-effects-candidate.sh`: 実executeの失敗/発散効果、実step再抽出・6本体借用検査・24否定対照を確認する。実stepのGAT拒否は継続する。
- `audit_revm_execute_llbc.py`: 実executeの関数値/Context由来、元のself型引数代入、全本体/unwind、透明実構造体と残存GATを監査し、11件のコピー証跡改変を拒否する。
- `verify-revm-execute-dynamic.sh`: 実execute生成本体の3全入力定理を、再抽出byte一致・厳格Lean・公理監査・leanchecker・16否定対照で検証する。Rust callback/heap/抽出器/全EVM対応は未証明。

- `build-gat-source-candidate.sh`: 元GATの型付きbinder/default/implied境界を保持するPure準備候補を分離ビルドし、強制MLテスト後に基準ソースを復元する。GAT抽出の厳格拒否は維持する。
- `verify-gat-source-candidate.sh`: 実GAT情報保持・3未対応形拒否・空判定fixture・実step再抽出/借用検査/GAT拒否を確認する。別途再抽出した実executeは既存の証明済み生成物とbyte一致する。新しい実行定理はない。

- `audit_memory_slice_len_llbc.py`: 実MemoryTr::slice_lenの共有receiver/返却寿命、checked offset加算、Range、slice呼出し、GAT情報、透明Refフィールドを監査し、2経路で12証跡改変を拒否する。
- `diagnose-memory-slice-len.sh`: 本番lockedソースから実slice_lenをopaque/透明Refの2経路で新規抽出する。両経路の借用検査成功とLeanのGAT厳格拒否を再現する。内部std/trait実装・全メモリ意味論の証明ではない。

- `audit_std_borrow_counter_llbc.py`: 固定stdの実is_reading/is_writingとUNUSED初期化を、Isize型・入力/global由来・比較の極性・全本体形で監査し、8証跡改変を拒否する。
- `verify-std-borrow-counter.sh`: 実生成std判定の全入力4定理を、新規抽出byte一致・厳格Lean・公理監査・leanchecker・13否定対照と3正対照で検証する。RefCell heapやRust/プラットフォーム対応は未証明。

`bash scripts/diagnose-std-borrow-ref-new.sh` は固定stdの実BorrowRef::newを本番manifestから2経路で新規抽出する診断ゲート。Opaque Cell経路の借用検査・Lean生成/コンパイル成功を確認した上で、Cell型/get/replaceの未証明3公理を監査で拒否する。透明Cell/UnsafeCell経路では実raw pointer dereferenceによる借用検査・Lean生成の拒否と成果物なしを確認し、12件のコピー証跡変更も拒否する。隔離生成物を証明済みtargetに含めず、新規heap/Rust対応定理は0件。範囲は `proofs/extraction/tool-patches/std-borrow-ref-new-profile.json`。

`bash scripts/verify-std-mem-replace-state.sh` は実std mem::replaceの全15操作列を新規抽出し、限定scalarヒープIRの7条件付き定理を検証する。別名をallocation/offsetで同じheapへ接続し、旧値返却・新値書込・別名観測・他Address不変・モデルfault時不変を含む。8schema拒否/2operand反映、公理監査/leanchecker、3禁止公理拒否/2偽観測拒否/2正観測も確認する。元Rust/ポインタ/retag/Cellとの意味論対応は未証明で、Rust対応定理0件。範囲は `proofs/extraction/tool-patches/std-mem-replace-state-profile.json`。

`bash scripts/verify-std-unsafe-cell-pointer.sh` は実std UnsafeCell::getのlayout/cast元情報と全19文を新規抽出し、scalarポインタIRの4定理を検証する。address/castを任意の状態効果として保持する効果合成と、明示契約下のpointer/heap保存を含む。7schema拒否、公理監査/leanchecker、3禁止公理拒否、契約なしのcast変更/失敗を保持する2正対照と、identity/成功を勝手に主張する2偽対照を確認する。物理Rust/layout/provenance/Cellとの意味論対応は未証明でRust対応定理0件。範囲は `proofs/extraction/tool-patches/std-unsafe-cell-pointer-profile.json`。

`bash scripts/verify-std-cell-get-state.sh` は実Cell::getの10 main文と2文unwind、layout/metadata/型代入・実UnsafeCell::getの19文を同一新規抽出から監査し、両生成物のbyte一致を検査する。caller/calleeの状態を接続したscalar IRの5主定理と1helper、9schema拒否、公理監査/leanchecker、3禁止公理拒否、2状態変更の正対照/2偽観測拒否を含む。物理field参照・read・retag/provenance・Rust unwind/UBの意味論対応は未証明で、Rust/Cell対応定理0件。範囲は `proofs/extraction/tool-patches/std-cell-get-state-profile.json`。

`bash scripts/diagnose-std-cell-replace.sh` は固定実Cell::replaceの全main28文/nested unwind20文、新値Move前のdrop flagクリア、Mut/TwoPhaseMut、元Destruct trait、Drop中の再unwind terminateを新規抽出で監査する。11コピー変更を拒否し、既存Cell get/UnsafeCell get/mem replaceの3生成IRとのbyte一致も確認する。成功経路だけのIRやdrop無効果を仮定せず、新規形式定理0件。範囲は `proofs/extraction/tool-patches/std-cell-replace-protocol-profile.json`。

`scripts/run_query_tx_e2e.sh` builds the gateway and ordinary-query price fixture, then verifies replicated query, storage and payment in one real tx through PocketIC 12. Set `POCKET_IC_BIN` to a compatible binary.
