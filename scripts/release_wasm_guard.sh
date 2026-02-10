#!/usr/bin/env bash
# where: local/CI release guard
# what: ensure production wasm endpoint check and metadata injection succeed
# why: keep release artifact reproducible and candid-synced
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

echo "[guard] build release wasm"
cargo build --target wasm32-unknown-unknown --release -p ic-evm-wrapper --locked

WASM_IN="target/wasm32-unknown-unknown/release/ic_evm_wrapper.wasm"
WASM_OUT="target/wasm32-unknown-unknown/release/ic_evm_wrapper.release.final.wasm"

echo "[guard] postprocess release wasm"
scripts/build_wasm_postprocess.sh "${WASM_IN}" "${WASM_OUT}"

echo "[guard] release wasm endpoint guard passed"
