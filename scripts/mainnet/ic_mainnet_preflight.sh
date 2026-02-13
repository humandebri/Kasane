#!/usr/bin/env bash
# where: mainnet deploy preparation
# what: run minimum preflight checks before ic deploy
# why: reduce production deployment mistakes (identity/controller/cycles/artifact)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

ICP_ENV="${ICP_ENV:-ic}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
CANISTER_ID="${CANISTER_ID:-}"
ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME:-ci-local}"
MIN_CYCLES="${MIN_CYCLES:-2000000000000}"
RUN_RELEASE_GUARD="${RUN_RELEASE_GUARD:-1}"

log() {
  echo "[ic-preflight] $*"
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[ic-preflight] missing command: $1" >&2
    exit 1
  fi
}

run_icp_canister() {
  if [[ -n "${ICP_IDENTITY_NAME}" ]]; then
    icp canister "$@" -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}"
  else
    icp canister "$@" -e "${ICP_ENV}"
  fi
}

status_target() {
  if [[ -n "${CANISTER_ID}" ]]; then
    echo "${CANISTER_ID}"
  else
    echo "${CANISTER_NAME}"
  fi
}

extract_balance_cycles() {
  local status_text="$1"
  printf "%s\n" "${status_text}" \
    | awk '
      /Balance:/ || /Cycles:/ {
        line = $0
        sub(/^.*: */, "", line)
        gsub(/_/, "", line)
        if (line ~ /^[0-9]+$/) {
          print line
          exit
        }
      }
    '
}

extract_controllers_line() {
  local status_text="$1"
  printf "%s\n" "${status_text}" \
    | awk '
      /Controllers:/ {
        line = $0
        sub(/^.*Controllers: */, "", line)
        print line
        exit
      }
    '
}

require_cmd icp
require_cmd cargo
require_cmd sed
require_cmd tr

log "environment=${ICP_ENV}"
log "target=$(status_target)"

if [[ -n "${ICP_IDENTITY_NAME}" ]]; then
  log "identity=${ICP_IDENTITY_NAME}"
fi

if [[ "${RUN_RELEASE_GUARD}" == "1" ]]; then
  log "running release guard"
  scripts/release_wasm_guard.sh
fi

TARGET="$(status_target)"

log "identity principal"
CURRENT_PRINCIPAL=""
if [[ -n "${ICP_IDENTITY_NAME}" ]]; then
  CURRENT_PRINCIPAL="$(icp identity principal --identity "${ICP_IDENTITY_NAME}")"
else
  CURRENT_PRINCIPAL="$(icp identity principal)"
fi
printf "%s\n" "${CURRENT_PRINCIPAL}"

log "canister status"
set +e
STATUS_TEXT="$(run_icp_canister status "${TARGET}" 2>&1)"
STATUS_CODE=$?
set -e
if [[ "${STATUS_CODE}" -ne 0 ]]; then
  echo "${STATUS_TEXT}" >&2
  echo "[ic-preflight] canister status failed. If this is first deploy, create canister first." >&2
  exit "${STATUS_CODE}"
fi
printf "%s\n" "${STATUS_TEXT}"

CONTROLLERS_LINE="$(extract_controllers_line "${STATUS_TEXT}")"
if [[ -n "${CURRENT_PRINCIPAL}" ]] && [[ -n "${CONTROLLERS_LINE}" ]]; then
  if ! grep -Fq "${CURRENT_PRINCIPAL}" <<<"${CONTROLLERS_LINE}"; then
    echo "[ic-preflight] selected identity is not a controller of target canister" >&2
    echo "[ic-preflight] current_principal=${CURRENT_PRINCIPAL}" >&2
    echo "[ic-preflight] controllers=${CONTROLLERS_LINE}" >&2
    echo "[ic-preflight] set ICP_IDENTITY_NAME to a controller identity and retry" >&2
    exit 1
  fi
fi

BALANCE="$(extract_balance_cycles "${STATUS_TEXT}")"
if [[ -z "${BALANCE}" ]]; then
  echo "[ic-preflight] cycles balance is not visible in canister status output" >&2
  echo "[ic-preflight] if this is mainnet, the selected identity is likely not a controller" >&2
  echo "[ic-preflight] set ICP_IDENTITY_NAME to a controller identity and retry" >&2
  exit 1
else
  python - <<PY
balance = int("${BALANCE}")
minimum = int("${MIN_CYCLES}")
if balance < minimum:
    raise SystemExit(f"[ic-preflight] cycles too low: {balance} < {minimum}")
print(f"[ic-preflight] cycles check: {balance} >= {minimum}")
PY
fi

log "preflight passed"
