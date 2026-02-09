#!/usr/bin/env bash
# where: pre-deploy smoke orchestration
# what: run canister build + rpc smoke + indexer smoke in one command
# why: keep release gating repeatable and easy to audit
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

RUN_DEPLOY="${RUN_DEPLOY:-0}"
RUN_INDEXER_SMOKE="${RUN_INDEXER_SMOKE:-1}"
NETWORK="${NETWORK:-local}"

echo "[predeploy] cargo check"
cargo check --workspace

echo "[predeploy] build wasm release"
cargo build -p ic-evm-wrapper --target wasm32-unknown-unknown --release

if [[ "$RUN_DEPLOY" == "1" ]]; then
  echo "[predeploy] local deploy"
  scripts/playground_manual_deploy.sh
fi

echo "[predeploy] rpc smoke"
NETWORK="$NETWORK" scripts/rpc_compat_smoke.sh

if [[ "$RUN_INDEXER_SMOKE" == "1" ]]; then
  echo "[predeploy] indexer smoke"
  scripts/local_indexer_smoke.sh
fi

echo "[predeploy] done"
