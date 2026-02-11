#!/usr/bin/env bash
# where: mainnet deploy helper
# what: build release wasm and install/upgrade evm canister on ic environment
# why: make production deploy explicit, repeatable, and auditable
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

source "${REPO_ROOT}/scripts/lib_init_args.sh"

ICP_ENV="${ICP_ENV:-ic}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
CANISTER_ID="${CANISTER_ID:-}"
ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME:-}"
MODE="${MODE:-upgrade}"
CREATE_IF_MISSING="${CREATE_IF_MISSING:-0}"
CONFIRM="${CONFIRM:-1}"
GENESIS_PRINCIPAL_AMOUNT="${GENESIS_PRINCIPAL_AMOUNT:-1000000000000000000}"
GENESIS_ETH_PRIVKEY="${GENESIS_ETH_PRIVKEY:-}"
GENESIS_ETH_AMOUNT="${GENESIS_ETH_AMOUNT:-1000000000000000000}"
WASM_PATH="${WASM_PATH:-target/wasm32-unknown-unknown/release/ic_evm_wrapper.release.final.wasm}"

log() {
  echo "[ic-deploy] $*"
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[ic-deploy] missing command: $1" >&2
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

target() {
  if [[ -n "${CANISTER_ID}" ]]; then
    echo "${CANISTER_ID}"
  else
    echo "${CANISTER_NAME}"
  fi
}

ensure_mode() {
  case "${MODE}" in
    install|reinstall|upgrade) ;;
    *)
      echo "[ic-deploy] unsupported MODE=${MODE} (install|reinstall|upgrade only)" >&2
      exit 1
      ;;
  esac
}

build_init_args() {
  if [[ -n "${GENESIS_ETH_PRIVKEY}" ]]; then
    build_init_args_for_current_identity_with_eth_sender \
      "${GENESIS_ETH_PRIVKEY}" \
      "${GENESIS_PRINCIPAL_AMOUNT}" \
      "${GENESIS_ETH_AMOUNT}"
  else
    build_init_args_for_current_identity "${GENESIS_PRINCIPAL_AMOUNT}"
  fi
}

confirm_or_abort() {
  if [[ "${CONFIRM}" != "1" ]]; then
    return
  fi
  if [[ ! -t 0 ]]; then
    echo "[ic-deploy] CONFIRM=1 requires tty. set CONFIRM=0 for non-interactive use." >&2
    exit 1
  fi
  echo "[ic-deploy] target environment: ${ICP_ENV}"
  echo "[ic-deploy] install mode: ${MODE}"
  echo "[ic-deploy] target canister: $(target)"
  echo "[ic-deploy] type YES to continue:"
  local ans
  read -r ans
  if [[ "${ans}" != "YES" ]]; then
    echo "[ic-deploy] aborted" >&2
    exit 1
  fi
}

require_cmd icp
require_cmd cargo
require_cmd python

ensure_mode
TARGET="$(target)"

if [[ -n "${ICP_IDENTITY_NAME}" ]]; then
  log "identity=${ICP_IDENTITY_NAME}"
fi
log "environment=${ICP_ENV}"
log "target=${TARGET}"
log "mode=${MODE}"

if [[ "${CREATE_IF_MISSING}" == "1" && -z "${CANISTER_ID}" ]]; then
  log "creating canister when missing: ${CANISTER_NAME}"
  run_icp_canister create "${CANISTER_NAME}" >/dev/null 2>&1 || true
fi

log "running release guard build (includes postprocess)"
scripts/release_wasm_guard.sh

if [[ ! -f "${WASM_PATH}" ]]; then
  echo "[ic-deploy] wasm not found: ${WASM_PATH}" >&2
  exit 1
fi
log "wasm=$(realpath "${WASM_PATH}")"

confirm_or_abort

if [[ "${MODE}" == "upgrade" ]]; then
  log "install upgrade (no init args)"
  run_icp_canister install \
    --mode "${MODE}" \
    --wasm "${WASM_PATH}" \
    "${TARGET}"
else
  INIT_ARGS="$(build_init_args)"
  log "install ${MODE} with init args"
  run_icp_canister install \
    --mode "${MODE}" \
    --wasm "${WASM_PATH}" \
    --args "${INIT_ARGS}" \
    "${TARGET}"
fi

log "post status"
run_icp_canister status "${TARGET}"

log "deploy done"
