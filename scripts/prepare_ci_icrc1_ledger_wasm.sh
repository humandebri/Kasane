#!/usr/bin/env bash
# where: GitHub Actions / CI helper
# what: vendored official ICRC-1 ledger wasm を選んで ICP_LEDGER_WASM を確定する
# why: wrap/unwrap E2E が network download に依存せず同じ ledger wasm を使うため
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT_DIR}/scripts/lib_ledger_artifact.sh"
LEDGER_RELEASE="${LEDGER_RELEASE:-ledger-suite-icrc-2026-03-09}"
LEDGER_RELEASE_RESOLVED="$(ledger_artifact_resolve_release "${LEDGER_RELEASE}")"
LEDGER_WASM="$(ledger_artifact_wasm_path "${ROOT_DIR}" "${LEDGER_RELEASE_RESOLVED}")"

if [[ ! -f "${LEDGER_WASM}" ]]; then
  echo "[prepare-ci-ledger] missing vendored ledger wasm: ${LEDGER_WASM}" >&2
  echo "[prepare-ci-ledger] commit third_party/dfinity/${LEDGER_RELEASE_RESOLVED}/ic-icrc1-ledger.wasm first" >&2
  exit 1
fi

echo "[prepare-ci-ledger] using ledger wasm at ${LEDGER_WASM}"
export ICP_LEDGER_WASM="${LEDGER_WASM}"

if [[ -n "${GITHUB_ENV:-}" ]]; then
  echo "ICP_LEDGER_WASM=${LEDGER_WASM}" >> "${GITHUB_ENV}"
fi
