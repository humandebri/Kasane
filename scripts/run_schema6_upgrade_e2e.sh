#!/usr/bin/env bash
# Reconstruct a historical schema-6 candidate, not the unidentified deployed Wasm.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"
COMMIT=d37340e3e80ace2803e667a0ec50ede032b4baba
SOURCE="${REPO_ROOT}/target/upgrade-history/${COMMIT}"
OLD_TARGET="${REPO_ROOT}/target/schema6-build"
FINAL_WASM="${EVM_GATEWAY_WASM:-${REPO_ROOT}/target/wasm32-unknown-unknown/release/ic_evm_gateway.release.final.wasm}"
[[ -f "${FINAL_WASM}" ]] || { echo 'Run scripts/release_wasm_guard.sh first' >&2; exit 1; }
mkdir -p "${SOURCE}"
git archive "${COMMIT}" | tar -x -C "${SOURCE}"
# The historical manifest uses candid but omitted that edge from Cargo.lock.
patch -d "${SOURCE}" -p1 < scripts/patches/schema6-d37340e3-lock.patch
CARGO_TARGET_DIR="${OLD_TARGET}" cargo build --locked --release \
  --manifest-path "${SOURCE}/Cargo.toml" --target wasm32-unknown-unknown -p ic-evm-gateway
OLD_WASM="${OLD_TARGET}/wasm32-unknown-unknown/release/ic_evm_gateway.wasm"
shasum -a 256 "${OLD_WASM}" "${FINAL_WASM}"
EVM_GATEWAY_INITIAL_WASM="${OLD_WASM}" EVM_GATEWAY_WASM="${FINAL_WASM}" \
CARGO_TARGET_DIR="${REPO_ROOT}/target/e2e-lifecycle" cargo test --locked \
  --manifest-path crates/evm-rpc-e2e/Cargo.toml --test rpc_compat_e2e \
  upgrade_and_snapshot_restore_preserve_executed_transaction_state \
  -- --exact --test-threads=1 --nocapture
