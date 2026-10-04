#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"
cargo build -p ic-evm-gateway --target wasm32-unknown-unknown --release --lib --example query_price_oracle
cargo test --manifest-path crates/evm-rpc-e2e/Cargo.toml --test query_tx_e2e -- --test-threads=1
