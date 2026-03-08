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
scripts/ci-local.sh github
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

## By Purpose

### Pre-checks and Quality Gates
- `scripts/ci-local.sh`: runs in `github|smoke|all` modes
- `scripts/check_gateway_api_compat_baseline.sh`: detects breaking changes in gateway API compatibility baseline (`--update` updates baseline)
- `scripts/check_gateway_matrix_sync.sh`: verifies compatibility matrix row in `tools/rpc-gateway/README.md` matches `tools/rpc-gateway/package.json` version line
- `scripts/predeploy_smoke.sh`: `cargo check` + wasm build + PocketIC RPC compatibility E2E (optional indexer smoke)
- `scripts/run_rpc_compat_e2e.sh`: RPC compatibility E2E test (`cargo test --test rpc_compat_e2e`)

### rpc-gateway Documentation Language Policy
- `tools/rpc-gateway/README.md` is the English canonical document
- Japanese supplement is `tools/rpc-gateway/README.ja.md` (same for `ops/`, `smoke/`, and `contracts/`)

### Local Operations
- `scripts/icp_local_clean_start.sh`: clean start helper for managed local network (`icp network`)
- `scripts/local_pruning_stage.sh`: staged pruning verification
- `scripts/local_indexer_fault_injection.sh`: indexer fault-injection test

### Playground
- `scripts/playground_manual_deploy.sh`: manual deployment to playground
- `scripts/playground_smoke.sh`: end-to-end Tx/RPC checks on playground
  - set `FUNDED_ETH_PRIVKEY` for additional transfer checks

### Mainnet Operations
- `scripts/mainnet/ic_mainnet_preflight.sh`: minimum pre-mainnet checks
- `scripts/mainnet/ic_mainnet_deploy.sh`: main deployment script
- `scripts/mainnet/ic_mainnet_post_upgrade_smoke.sh`: minimum RPC checks after deploy
- `scripts/verify_submit_after_deploy.sh`: manual/CI hook for verify submit
- `scripts/mainnet/mainnet_method_test.sh`: heavy mainnet method test
  - `MINING_IDLE_OBSERVE_SEC`: idle observation seconds at start (default: `6`)
  - `IDLE_MAX_CYCLE_DELTA`: allowed cycle decrease in idle observation. `0` disables threshold check (default: `0`)

### Prune Operations
- `scripts/ops/apply_prune_policy.sh`: apply policy + enable pruning + status check
- `scripts/ops/tune_prune_max_ops.sh`: staged tuning based on need_prune/error counters
- `scripts/ops/test_prune_ops_scripts.sh`: mock tests for the two scripts above
- `scripts/ops/contabo_deploy_tools.sh`: sync `tools/indexer` / `tools/explorer` from git worktree on Contabo, then build+restart (git ref based)
- `scripts/ops/contabo_deploy_gateway.sh`: sync `tools/rpc-gateway` from git worktree on Contabo, then build+restart (git ref based)

## Key Environment Variables
- `CANISTER_NAME` / `CANISTER_ID`
- `WRAP_CANISTER_ID`
  - required for scripts that build `InitArgs` (`build_init_args_for_current_identity`)
  - `install` / `reinstall` flows no longer auto-resolve `wrap_canister`
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
