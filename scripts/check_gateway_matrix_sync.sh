#!/usr/bin/env bash
# where: local/CI guard
# what: ensure gateway compatibility matrix row matches package version
# why: prevent README matrix drift from actual gateway release line

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
GATEWAY_PACKAGE_JSON="${REPO_ROOT}/tools/rpc-gateway/package.json"
GATEWAY_README="${REPO_ROOT}/tools/rpc-gateway/README.md"

if ! command -v node >/dev/null 2>&1; then
  echo "[guard] node is required: install Node.js before running scripts/check_gateway_matrix_sync.sh" >&2
  exit 1
fi

if [[ ! -f "${GATEWAY_PACKAGE_JSON}" || ! -f "${GATEWAY_README}" ]]; then
  echo "[guard] required file missing" >&2
  exit 1
fi

GATEWAY_VERSION="$(node -e 'const fs=require("fs");const p=JSON.parse(fs.readFileSync(process.argv[1],"utf8"));process.stdout.write(p.version);' "${GATEWAY_PACKAGE_JSON}")"
MAJOR_MINOR_X="$(echo "${GATEWAY_VERSION}" | awk -F. '{print $1 "." $2 ".x"}')"
EXPECTED="\`ic-evm-rpc-gateway@${MAJOR_MINOR_X}\`"

if ! grep -Fq "${EXPECTED}" "${GATEWAY_README}"; then
  echo "[guard] compatibility matrix gateway_version mismatch" >&2
  echo "[guard] expected to include: ${EXPECTED}" >&2
  echo "[guard] package version: ${GATEWAY_VERSION}" >&2
  exit 1
fi

echo "[guard] gateway matrix sync ok (${EXPECTED})"
