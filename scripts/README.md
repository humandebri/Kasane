# scripts/README.md

Japanese version: [./README.ja.md](./README.ja.md)

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

### rpc-gateway Documentation Language Policy
- `tools/rpc-gateway/README.md` is the English canonical document
- Japanese supplement is `tools/rpc-gateway/README.ja.md` (same for `ops/`, `smoke/`, and `contracts/`)

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
- `scripts/ops/contabo_deploy_tools.sh`: sync `tools/indexer` / `tools/explorer` from git worktree on Contabo, then build+restart (git ref based)
- `scripts/ops/contabo_deploy_gateway.sh`: sync `tools/rpc-gateway` from git worktree on Contabo, then build+restart (git ref based)
  - when Contabo cannot use GitHub HTTPS auth, pass `REPO_URL=git@github.com:<owner>/<repo>.git` and give the remote `deployer` user a read-only SSH deploy key
  - remote clone/fetch is expected to run as `deployer`; avoid provisioning the deploy key only for `root`

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
