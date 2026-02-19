#!/usr/bin/env bash
# where: deploy後のverify submit補助
# what: explorer verify submitスクリプトを安全に呼び出す
# why: deployパイプラインに1行追加でverify自動化できるようにするため
set -euo pipefail

AUTO_VERIFY_SUBMIT="${AUTO_VERIFY_SUBMIT:-0}"

log() {
  echo "[verify-submit-hook] $*"
}

if [[ "${AUTO_VERIFY_SUBMIT}" != "1" ]]; then
  log "skip (AUTO_VERIFY_SUBMIT!=1)"
  exit 0
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXPLORER_DIR="${ROOT_DIR}/tools/explorer"

if [[ ! -d "${EXPLORER_DIR}" ]]; then
  log "explorer dir not found: ${EXPLORER_DIR}"
  exit 1
fi

if [[ -z "${VERIFY_PAYLOAD_FILE:-}" ]]; then
  log "VERIFY_PAYLOAD_FILE is required when AUTO_VERIFY_SUBMIT=1"
  exit 1
fi
if [[ -z "${VERIFY_AUTH_KID:-}" ]]; then
  log "VERIFY_AUTH_KID is required when AUTO_VERIFY_SUBMIT=1"
  exit 1
fi
if [[ -z "${VERIFY_AUTH_SECRET:-}" ]]; then
  log "VERIFY_AUTH_SECRET is required when AUTO_VERIFY_SUBMIT=1"
  exit 1
fi

if [[ ! -f "${VERIFY_PAYLOAD_FILE}" ]]; then
  log "payload file not found: ${VERIFY_PAYLOAD_FILE}"
  exit 1
fi

log "submit verify payload: ${VERIFY_PAYLOAD_FILE}"
(
  cd "${EXPLORER_DIR}"
  VERIFY_SUBMIT_URL="${VERIFY_SUBMIT_URL:-http://localhost:3000/api/verify/submit}" \
  VERIFY_PAYLOAD_FILE="${VERIFY_PAYLOAD_FILE}" \
  VERIFY_AUTH_KID="${VERIFY_AUTH_KID}" \
  VERIFY_AUTH_SECRET="${VERIFY_AUTH_SECRET}" \
  VERIFY_AUTH_SUB="${VERIFY_AUTH_SUB:-deploy-bot}" \
  VERIFY_AUTH_SCOPE="${VERIFY_AUTH_SCOPE:-verify.submit}" \
  VERIFY_AUTH_TTL_SEC="${VERIFY_AUTH_TTL_SEC:-300}" \
  npm run verify:submit
)

log "verify submit done"
