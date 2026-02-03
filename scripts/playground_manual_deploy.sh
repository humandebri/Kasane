#!/usr/bin/env bash
# where: playground-only manual build/deploy helper
# what: build wasm with cargo (no dfx build) and install via dfx
# why: keep wasm small and reproducible; avoid dfx-internal build/strip issues
# note: this is intended for playground use only (size issues); not needed for mainnet deploys
set -euo pipefail
source "$(dirname "$0")/lib_init_args.sh"

NETWORK="${NETWORK:-playground}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
CANISTER_ID="${CANISTER_ID:-}"
MODE="${MODE:-reinstall}"
ENABLE_DEV_FAUCET="${ENABLE_DEV_FAUCET:-0}"
RUN_SMOKE="${RUN_SMOKE:-0}"

DFX="dfx canister --network ${NETWORK}"

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
  local wasm_out="target/wasm32-unknown-unknown/release/ic_evm_wrapper.candid.wasm"
  if ! command -v ic-wasm >/dev/null 2>&1; then
    log "installing ic-wasm"
    cargo install ic-wasm --locked
  fi
  ic-wasm "${wasm_path}" -o "${wasm_out}" metadata candid:service -f crates/ic-evm-wrapper/evm_canister.did

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
    ${DFX} install --mode "${MODE}" --wasm "${wasm_out}" --argument "${init_args}" "${CANISTER_ID}"
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
    ${DFX} install --mode "${MODE}" --wasm "${wasm_out}" --argument "${init_args}" "${CANISTER_NAME}"
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
