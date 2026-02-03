#!/usr/bin/env bash
# where: local/manual CI helper
# what: build latest wrapper wasm, then run rpc_compat_e2e with pocket-ic
# why: prevent stale wasm causing false e2e failures

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
POCKET_IC_BIN="${POCKET_IC_BIN:-${REPO_ROOT}/crates/evm-rpc-e2e/pocket-ic}"

cargo build -p ic-evm-wrapper --target wasm32-unknown-unknown --release
(
  cd "${REPO_ROOT}/crates/evm-rpc-e2e"
  POCKET_IC_BIN="${POCKET_IC_BIN}" cargo test --test rpc_compat_e2e -- --test-threads=1
)
