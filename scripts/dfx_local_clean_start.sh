#!/usr/bin/env bash
# where: local dfx recovery bootstrap
# what: kill lingering dfx/replica/icx-proxy, clear .dfx/local, start dfx in foreground with logs
# why: 503/timeoutやdfx stopハングを根治してから検証を進めるため
set -euo pipefail

LOG_DIR="${LOG_DIR:-/tmp/dfx-logs}"
DFX_START_LOG="${DFX_START_LOG:-${LOG_DIR}/dfx_start.log}"
DFX_LOCAL_DIR="${DFX_LOCAL_DIR:-.dfx/local}"

log() {
  echo "[dfx-local-clean-start] $*"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "[dfx-local-clean-start] missing command: $1" >&2
    exit 1
  }
}

require_cmd dfx
require_cmd pkill
require_cmd tee

mkdir -p "${LOG_DIR}"

log "stop dfx and kill lingering processes"
dfx stop >/dev/null 2>&1 || true
pkill -f dfx >/dev/null 2>&1 || true
pkill -f replica >/dev/null 2>&1 || true
pkill -f icx-proxy >/dev/null 2>&1 || true

log "remove local state: ${DFX_LOCAL_DIR}"
rm -rf "${DFX_LOCAL_DIR}"

log "starting dfx in foreground; keep this terminal open"
log "log path: ${DFX_START_LOG}"
log "health check: curl -m 2 -sSf http://127.0.0.1:4943/api/v2/status"

dfx start --clean 2>&1 | tee "${DFX_START_LOG}"
