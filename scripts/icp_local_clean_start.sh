#!/usr/bin/env bash
# where: local icp-cli recovery bootstrap
# what: stop the managed local network, restart it in foreground-equivalent background mode, and print health info
# why: v0.2.0 の managed network 前提で local API を最短復旧するため
set -euo pipefail

NETWORK="${NETWORK:-local}"

log() {
  echo "[icp-local-clean-start] $*"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "[icp-local-clean-start] missing command: $1" >&2
    exit 1
  }
}

require_cmd icp
require_cmd python

log "stop managed network: ${NETWORK}"
icp network stop "${NETWORK}" >/dev/null 2>&1 || true

log "start managed network in background: ${NETWORK}"
icp network start "${NETWORK}" -d

STATUS_JSON="$(icp network status "${NETWORK}" --json)"
API_URL="$(STATUS_JSON="${STATUS_JSON}" python - <<'PY'
import json
import os

data = json.loads(os.environ["STATUS_JSON"])
api_url = data.get("api_url")
if isinstance(api_url, str) and api_url:
    print(api_url)
else:
    port = data.get("port")
    print(f"http://127.0.0.1:{port}" if isinstance(port, int) and port > 0 else "http://127.0.0.1:8000")
PY
)"

log "health check: curl -m 2 -sSf ${API_URL}/api/v2/status"
log "status: ${STATUS_JSON}"
