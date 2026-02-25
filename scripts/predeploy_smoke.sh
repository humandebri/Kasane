#!/usr/bin/env bash
# where: pre-deploy smoke orchestration
# what: run canister build + PocketIC RPC e2e (+ optional local indexer smoke)
# why: keep release gating repeatable and avoid local deploy dependencies
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

RUN_INDEXER_SMOKE="${RUN_INDEXER_SMOKE:-0}"

echo "[predeploy] cargo check"
cargo check --workspace

echo "[predeploy] build wasm release"
cargo build -p ic-evm-wrapper --target wasm32-unknown-unknown --release

echo "[predeploy] rpc compat e2e (PocketIC)"
scripts/run_rpc_compat_e2e.sh

if [[ "$RUN_INDEXER_SMOKE" == "1" ]]; then
  echo "[predeploy] indexer smoke (local network)"
  scripts/local_indexer_smoke.sh
fi

echo "[predeploy] done"
