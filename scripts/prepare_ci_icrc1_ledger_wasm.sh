#!/usr/bin/env bash
# where: GitHub Actions / CI helper
# what: official ICRC-1 ledger wasm を取得して ICP_LEDGER_WASM を確定する
# why: wrap/unwrap E2E が読む ledger wasm の由来を固定し、fallback を持たせないため
set -euo pipefail

LEDGER_RELEASE="${LEDGER_RELEASE:-ledger-suite-icrc-2026-03-09}"
LEDGER_CACHE_DIR="${LEDGER_CACHE_DIR:-/tmp/kasane-ledger-cache}"
LEDGER_WASM_GZ="${LEDGER_CACHE_DIR}/ic-icrc1-ledger.wasm.gz"
LEDGER_WASM="${LEDGER_CACHE_DIR}/ic-icrc1-ledger.wasm"

mkdir -p "${LEDGER_CACHE_DIR}"

if [[ ! -f "${LEDGER_WASM}" ]]; then
  echo "[prepare-ci-ledger] download official ledger wasm: ${LEDGER_RELEASE}"
  curl -L --fail \
    --output "${LEDGER_WASM_GZ}" \
    "https://github.com/dfinity/ic/releases/download/${LEDGER_RELEASE}/ic-icrc1-ledger.wasm.gz"
  gzip -dc "${LEDGER_WASM_GZ}" > "${LEDGER_WASM}"
fi

echo "[prepare-ci-ledger] using ledger wasm at ${LEDGER_WASM}"
export ICP_LEDGER_WASM="${LEDGER_WASM}"

if [[ -n "${GITHUB_ENV:-}" ]]; then
  echo "ICP_LEDGER_WASM=${LEDGER_WASM}" >> "${GITHUB_ENV}"
fi
