#!/usr/bin/env bash
# where: local query smoke helper
# what: run query checks via @dfinity/agent (Actor query), not icp canister call
# why: icp canister call is update-only and clashes with inspect_message policy
set -euo pipefail

NETWORK="${NETWORK:-local}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
CANISTER_ID="${CANISTER_ID:-}"
ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME:-ci-local}"
IC_HOST="${IC_HOST:-}"
INDEXER_DIR="${INDEXER_DIR:-tools/indexer}"
QUERY_SMOKE_MAX_BYTES="${QUERY_SMOKE_MAX_BYTES:-65536}"
QUERY_SMOKE_REQUIRED_HEAD_MIN="${QUERY_SMOKE_REQUIRED_HEAD_MIN:-}"
QUERY_SMOKE_ALLOW_EXPORT_MISSING_DATA="${QUERY_SMOKE_ALLOW_EXPORT_MISSING_DATA:-true}"
INDEXER_FETCH_ROOT_KEY="${INDEXER_FETCH_ROOT_KEY:-}"

if [[ ! -d "${INDEXER_DIR}/node_modules" ]]; then
  echo "[query-smoke] npm install (${INDEXER_DIR})"
  (cd "${INDEXER_DIR}" && npm install)
fi

resolve_canister_id() {
  if [[ -n "${CANISTER_ID}" ]]; then
    echo "${CANISTER_ID}"
    return
  fi
  icp canister status -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --id-only "${CANISTER_NAME}"
}

resolve_ic_host() {
  if [[ -n "${IC_HOST}" ]]; then
    echo "${IC_HOST}"
    return
  fi
  if [[ "${NETWORK}" == "ic" ]]; then
    echo "https://icp-api.io"
    return
  fi
  STATUS_JSON="$(icp network status "${NETWORK}" --json 2>/dev/null || true)" python - <<'PY'
import json
import os
import sys
text = os.environ.get("STATUS_JSON", "").strip()
if not text:
    print("http://127.0.0.1:4943")
    raise SystemExit(0)
try:
    data = json.loads(text)
except Exception:
    print("http://127.0.0.1:4943")
    raise SystemExit(0)
port = data.get("port")
print(f"http://127.0.0.1:{port}" if isinstance(port, int) and port > 0 else "http://127.0.0.1:4943")
PY
}

resolve_fetch_root_key() {
  if [[ -n "${INDEXER_FETCH_ROOT_KEY}" ]]; then
    echo "${INDEXER_FETCH_ROOT_KEY}"
    return
  fi
  if [[ "${NETWORK}" == "ic" ]]; then
    echo "false"
  else
    echo "true"
  fi
}

RESOLVED_CANISTER_ID="$(resolve_canister_id)"
RESOLVED_IC_HOST="$(resolve_ic_host)"
RESOLVED_FETCH_ROOT_KEY="$(resolve_fetch_root_key)"

echo "[query-smoke] network=${NETWORK} canister=${RESOLVED_CANISTER_ID} host=${RESOLVED_IC_HOST} identity=${ICP_IDENTITY_NAME} fetch_root_key=${RESOLVED_FETCH_ROOT_KEY}"
(
  cd "${INDEXER_DIR}"
  EVM_CANISTER_ID="${RESOLVED_CANISTER_ID}" \
  INDEXER_IC_HOST="${RESOLVED_IC_HOST}" \
  INDEXER_FETCH_ROOT_KEY="${RESOLVED_FETCH_ROOT_KEY}" \
  QUERY_SMOKE_MAX_BYTES="${QUERY_SMOKE_MAX_BYTES}" \
  QUERY_SMOKE_REQUIRED_HEAD_MIN="${QUERY_SMOKE_REQUIRED_HEAD_MIN}" \
  QUERY_SMOKE_ALLOW_EXPORT_MISSING_DATA="${QUERY_SMOKE_ALLOW_EXPORT_MISSING_DATA}" \
  ./node_modules/.bin/tsx src/query_smoke.ts
)
