#!/usr/bin/env bash
# どこで: wasmターゲット依存解決 / 何を: default build へ BLS/KZG backend 依存が流入していないか検証 / なぜ: 登録停止だけでなく重い依存そのものを外した状態を維持するため

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

TARGET="wasm32-unknown-unknown"
PACKAGE="ic-evm-core"

check_absent_dep() {
  local crate="$1"
  local tree_output
  local stripped

  tree_output="$(cargo tree -p "${PACKAGE}" --target "${TARGET}" -e normal -i "${crate}" 2>&1 || true)"
  stripped="$(printf '%s\n' "${tree_output}" | sed -E 's/\x1B\[[0-9;]*[[:alpha:]]//g')"

  if [[ -z "${stripped//[[:space:]]/}" ]] || grep -Fq "nothing to print" <<< "${stripped}"; then
    echo "[guard] pass: ${crate} is absent from default ${PACKAGE} ${TARGET} graph"
    return 0
  fi

  if grep -Fq "${crate} v" <<< "${stripped}"; then
    echo "[guard] failed: unexpected dependency present in default ${PACKAGE} ${TARGET} graph: ${crate}" >&2
    echo "${stripped}" >&2
    exit 1
  fi

  echo "[guard] failed: unexpected cargo tree output while checking ${crate}" >&2
  echo "${stripped}" >&2
  exit 1
}

check_absent_dep "ark-bls12-381"
check_absent_dep "c-kzg"
check_absent_dep "blst"
