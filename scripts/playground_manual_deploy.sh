#!/usr/bin/env bash
# where: playground-only manual build/deploy helper
# what: build wasm with cargo (no dfx build) and install via dfx
# why: keep wasm small and reproducible; avoid dfx-internal build/strip issues
# note: this is intended for playground use only (size issues); not needed for mainnet deploys
set -euo pipefail

NETWORK="${NETWORK:-playground}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
CANISTER_ID="${CANISTER_ID:-}"
MODE="${MODE:-reinstall}"
ENABLE_DEV_FAUCET="${ENABLE_DEV_FAUCET:-0}"
RUN_SMOKE="${RUN_SMOKE:-0}"

DFX="dfx --network ${NETWORK}"

log() {
  echo "[manual-deploy] $*"
}

build_wasm() {
  local -a args
  args=("--release" "--target" "wasm32-unknown-unknown" "-p" "ic-evm-wrapper")
  if [[ "${ENABLE_DEV_FAUCET}" == "1" ]]; then
    args+=("--features" "dev-faucet")
  fi
  log "cargo build ${args[*]}"
  cargo build "${args[@]}"
}

install_wasm() {
  local wasm_path="target/wasm32-unknown-unknown/release/ic_evm_wrapper.wasm"
  if [[ ! -f "${wasm_path}" ]]; then
    log "wasm not found: ${wasm_path}"
    exit 1
  fi
  log "wasm size: $(ls -lh "${wasm_path}" | awk '{print $5}')"

  if [[ "${NETWORK}" == "playground" || "${NETWORK}" == "ic" ]]; then
    if [[ -z "${CANISTER_ID}" ]]; then
      log "CANISTER_ID is required for network=${NETWORK}"
      exit 1
    fi
    log "install wasm to canister_id=${CANISTER_ID} mode=${MODE}"
    ${DFX} canister install --mode "${MODE}" --wasm "${wasm_path}" "${CANISTER_ID}"
  else
    log "install wasm to canister_name=${CANISTER_NAME} mode=${MODE}"
    ${DFX} canister install --mode "${MODE}" --wasm "${wasm_path}" "${CANISTER_NAME}"
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
