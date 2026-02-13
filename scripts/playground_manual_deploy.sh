#!/usr/bin/env bash
# where: playground-only manual build/deploy helper
# what: build wasm with cargo (no dfx build) and install via icp-cli
# why: keep wasm small and reproducible; avoid dfx-internal build/strip issues
# note: this is intended for playground use only (size issues); not needed for mainnet deploys
set -euo pipefail
source "$(dirname "$0")/lib_init_args.sh"

NETWORK="${NETWORK:-playground}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
CANISTER_ID="${CANISTER_ID:-}"
MODE="${MODE:-reinstall}"
RUN_SMOKE="${RUN_SMOKE:-0}"

log() {
  echo "[manual-deploy] $*"
}

build_wasm() {
  log "cargo build --release --target wasm32-unknown-unknown -p ic-evm-wrapper"
  cargo build --release --target wasm32-unknown-unknown -p ic-evm-wrapper
}

install_wasm() {
  local wasm_path="target/wasm32-unknown-unknown/release/ic_evm_wrapper.wasm"
  if [[ ! -f "${wasm_path}" ]]; then
    log "wasm not found: ${wasm_path}"
    exit 1
  fi
  log "wasm size: $(ls -lh "${wasm_path}" | awk '{print $5}')"
  local wasm_out="target/wasm32-unknown-unknown/release/ic_evm_wrapper.final.wasm"
  scripts/build_wasm_postprocess.sh "${wasm_path}" "${wasm_out}"

  if [[ "${NETWORK}" == "playground" || "${NETWORK}" == "ic" ]]; then
    if [[ -z "${CANISTER_ID}" ]]; then
      log "CANISTER_ID is required for network=${NETWORK}"
      exit 1
    fi
    log "install wasm to canister_id=${CANISTER_ID} mode=${MODE}"
    echo "This will ${MODE} the canister ${CANISTER_ID}. Type YES to continue:"
    read -r confirm
    if [[ "${confirm}" != "YES" ]]; then
      log "aborted"
      exit 1
    fi
    local init_args
    init_args="$(build_init_args_for_current_identity 1000000000000000000)"
    icp canister install -n "${NETWORK}" --mode "${MODE}" --wasm "${wasm_out}" --args "${init_args}" "${CANISTER_ID}"
  else
    log "install wasm to canister_name=${CANISTER_NAME} mode=${MODE}"
    echo "This will ${MODE} the canister ${CANISTER_NAME}. Type YES to continue:"
    read -r confirm
    if [[ "${confirm}" != "YES" ]]; then
      log "aborted"
      exit 1
    fi
    local init_args
    init_args="$(build_init_args_for_current_identity 1000000000000000000)"
    icp canister install -n "${NETWORK}" --mode "${MODE}" --wasm "${wasm_out}" --args "${init_args}" "${CANISTER_NAME}"
  fi
}

build_wasm
install_wasm

if [[ "${RUN_SMOKE}" == "1" ]]; then
  if [[ "${NETWORK}" != "playground" ]]; then
    log "RUN_SMOKE=1 is only wired for playground_smoke.sh (network=playground)"
    exit 1
  fi
  log "running playground smoke"
  CANISTER_ID="${CANISTER_ID}" scripts/playground_smoke.sh
fi
