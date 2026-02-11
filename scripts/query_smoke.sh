#!/usr/bin/env bash
# where: local query smoke helper
# what: run query checks via @dfinity/agent (Actor query), not icp canister call
# why: icp canister call is update-only and clashes with inspect_message policy
set -euo pipefail

NETWORK="${NETWORK:-local}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME:-ci-local}"
INDEXER_DIR="${INDEXER_DIR:-tools/indexer}"
QUERY_SMOKE_MAX_BYTES="${QUERY_SMOKE_MAX_BYTES:-65536}"
QUERY_SMOKE_REQUIRED_HEAD_MIN="${QUERY_SMOKE_REQUIRED_HEAD_MIN:-}"
QUERY_SMOKE_ALLOW_EXPORT_MISSING_DATA="${QUERY_SMOKE_ALLOW_EXPORT_MISSING_DATA:-true}"

if [[ ! -d "${INDEXER_DIR}/node_modules" ]]; then
  echo "[query-smoke] npm install (${INDEXER_DIR})"
  (cd "${INDEXER_DIR}" && npm install)
fi

CANISTER_ID="$(icp canister status -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --id-only "${CANISTER_NAME}")"
IC_HOST="$(STATUS_JSON="$(icp network status "${NETWORK}" --json 2>/dev/null || true)" python - <<'PY'
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
)"

echo "[query-smoke] network=${NETWORK} canister=${CANISTER_ID} host=${IC_HOST}"
(
  cd "${INDEXER_DIR}"
  EVM_CANISTER_ID="${CANISTER_ID}" \
  INDEXER_IC_HOST="${IC_HOST}" \
  INDEXER_FETCH_ROOT_KEY="true" \
  QUERY_SMOKE_MAX_BYTES="${QUERY_SMOKE_MAX_BYTES}" \
  QUERY_SMOKE_REQUIRED_HEAD_MIN="${QUERY_SMOKE_REQUIRED_HEAD_MIN}" \
  QUERY_SMOKE_ALLOW_EXPORT_MISSING_DATA="${QUERY_SMOKE_ALLOW_EXPORT_MISSING_DATA}" \
  ./node_modules/.bin/tsx src/query_smoke.ts
)
