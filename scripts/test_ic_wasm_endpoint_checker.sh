#!/usr/bin/env bash
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bash "${REPO_ROOT}/scripts/prepare_ic_wasm_endpoint_checker.sh" >/dev/null
TOOL_ROOT="${REPO_ROOT}/target/tools/ic-wasm-endpoint-checker"
CARGO_TARGET_DIR="${TOOL_ROOT}/build" cargo test --locked \
  --manifest-path "${TOOL_ROOT}/ic-wasm-0.11.1/Cargo.toml" --lib composite_query_tests
