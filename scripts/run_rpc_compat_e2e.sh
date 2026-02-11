#!/usr/bin/env bash
# where: local/manual CI helper
# what: build latest wrapper wasm, then run rpc_compat_e2e with pocket-ic
# why: prevent stale wasm causing false e2e failures

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
POCKET_IC_BIN="${POCKET_IC_BIN:-${REPO_ROOT}/crates/evm-rpc-e2e/pocket-ic}"
E2E_TIMEOUT_SECONDS="${E2E_TIMEOUT_SECONDS:-120}"

cargo build -p ic-evm-wrapper --target wasm32-unknown-unknown --release
(
  cd "${REPO_ROOT}/crates/evm-rpc-e2e"
  TEST_CMD=(cargo test --test rpc_compat_e2e -- --test-threads=1)
  if command -v gtimeout >/dev/null 2>&1; then
    POCKET_IC_BIN="${POCKET_IC_BIN}" gtimeout "${E2E_TIMEOUT_SECONDS}" "${TEST_CMD[@]}"
  elif command -v timeout >/dev/null 2>&1; then
    POCKET_IC_BIN="${POCKET_IC_BIN}" timeout "${E2E_TIMEOUT_SECONDS}" "${TEST_CMD[@]}"
  else
    echo "[warn] timeout command not found; running without timeout" >&2
    POCKET_IC_BIN="${POCKET_IC_BIN}" "${TEST_CMD[@]}"
  fi
)
