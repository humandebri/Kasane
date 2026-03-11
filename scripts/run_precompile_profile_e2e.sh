#!/usr/bin/env bash
# where: local/manual CI helper
# what: build gateway wasm and run PocketIC precompile profile e2e
# why: collect deterministic precompile instruction profiles without a live network

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
POCKET_IC_BIN="${POCKET_IC_BIN:-${REPO_ROOT}/crates/evm-rpc-e2e/pocket-ic}"
E2E_TIMEOUT_SECONDS="${E2E_TIMEOUT_SECONDS:-180}"
ADMIN_DID_FILE="${REPO_ROOT}/crates/ic-evm-gateway/evm_canister_precompile_profile_admin.did"
RAW_WASM="${REPO_ROOT}/target/wasm32-unknown-unknown/release/ic_evm_gateway.wasm"
FINAL_WASM="${REPO_ROOT}/target/wasm32-unknown-unknown/release/ic_evm_gateway.precompile_profile_admin.final.wasm"

cargo build -p ic-evm-gateway --target wasm32-unknown-unknown --release --features precompile-profile-admin
DID_FILE="${ADMIN_DID_FILE}" "${REPO_ROOT}/scripts/build_wasm_postprocess.sh" "${RAW_WASM}" "${FINAL_WASM}"
(
  cd "${REPO_ROOT}/crates/evm-rpc-e2e"
  TEST_CMD=(cargo test --test precompile_profile_e2e -- --nocapture --test-threads=1)
  if command -v gtimeout >/dev/null 2>&1; then
    IC_EVM_GATEWAY_WASM="${FINAL_WASM}" POCKET_IC_BIN="${POCKET_IC_BIN}" gtimeout "${E2E_TIMEOUT_SECONDS}" "${TEST_CMD[@]}"
  elif command -v timeout >/dev/null 2>&1; then
    IC_EVM_GATEWAY_WASM="${FINAL_WASM}" POCKET_IC_BIN="${POCKET_IC_BIN}" timeout "${E2E_TIMEOUT_SECONDS}" "${TEST_CMD[@]}"
  else
    echo "[warn] timeout command not found; running without timeout" >&2
    IC_EVM_GATEWAY_WASM="${FINAL_WASM}" POCKET_IC_BIN="${POCKET_IC_BIN}" "${TEST_CMD[@]}"
  fi
)
