#!/usr/bin/env bash
# どこで: wasmターゲット依存解決 / 何を: getrandom feature経路を可視化 / なぜ: custom backend前提の崩れを早期検知するため

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

if ! TREE_OUTPUT="$(cargo tree -p ic-evm-wrapper -i getrandom@0.2.17 --target wasm32-unknown-unknown -e features 2>&1)"; then
  echo "ERROR: failed to resolve wasm dependency graph for getrandom check."
  echo "$TREE_OUTPUT"
  exit 1
fi

if [[ -z "$TREE_OUTPUT" ]]; then
  echo "ERROR: getrandom@0.2.17 is not present in wasm dependency graph."
  exit 1
fi

if ! grep -q 'getrandom feature "custom"' <<< "$TREE_OUTPUT"; then
  echo "ERROR: getrandom custom backend is not enabled for wasm."
  echo "$TREE_OUTPUT"
  exit 1
fi

if grep -q 'getrandom feature "std"' <<< "$TREE_OUTPUT"; then
  echo "WARN: getrandom std feature is still present through transitive deps."
fi

echo "OK: getrandom custom backend is enabled for wasm."
