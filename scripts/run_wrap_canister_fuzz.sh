#!/usr/bin/env bash
# where: repo root
# what: build wrap-canister wasm and launch the standalone canfuzz runner
# why: make canister_fuzzing execution reproducible without touching the main workspace

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"
export WRAP_CANISTER_WASM="${WRAP_CANISTER_WASM:-$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/wrap_canister.wasm}"
export OUT_DIR="${OUT_DIR:-$CARGO_TARGET_DIR/canfuzz-out}"

if [[ -z "${POCKET_IC_BIN:-}" ]]; then
  for candidate in \
    "$ROOT_DIR/crates/evm-rpc-e2e/pocket-ic" \
    "$ROOT_DIR/.canbench/pocket-ic"
  do
    if [[ -x "$candidate" ]]; then
      export POCKET_IC_BIN="$candidate"
      break
    fi
  done
fi

cargo build --target wasm32-unknown-unknown --release -p wrap-canister
mkdir -p "$OUT_DIR"
cargo run --manifest-path "$ROOT_DIR/fuzz/wrap-canister-canfuzz/Cargo.toml" --release -- "$@"
