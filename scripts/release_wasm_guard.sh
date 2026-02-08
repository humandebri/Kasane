#!/usr/bin/env bash
# where: local/CI release guard
# what: ensure production wasm does not expose dev-faucet endpoint
# why: prevent accidental dev_mint exposure in production builds
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

echo "[guard] build release wasm without dev-faucet"
cargo build --target wasm32-unknown-unknown --release -p ic-evm-wrapper --locked

WASM_IN="target/wasm32-unknown-unknown/release/ic_evm_wrapper.wasm"
WASM_OUT="target/wasm32-unknown-unknown/release/ic_evm_wrapper.release.final.wasm"

echo "[guard] postprocess with REQUIRE_NO_DEV_FAUCET=1"
REQUIRE_NO_DEV_FAUCET=1 scripts/build_wasm_postprocess.sh "${WASM_IN}" "${WASM_OUT}"

echo "[guard] release wasm endpoint guard passed"
