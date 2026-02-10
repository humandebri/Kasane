#!/usr/bin/env bash
# where: staging profiling helper
# what: build and instrument wasm for profiling-focused deploys
# why: measure hot paths without mixing profiling binary into normal release flow
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
source "${SCRIPT_DIR}/lib_init_args.sh"

NETWORK="${NETWORK:-staging}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
CANISTER_ID="${CANISTER_ID:-}"
MODE="${MODE:-reinstall}"
TRACE_ONLY="${TRACE_ONLY:-produce_block,execute_tx_on}"
INPUT_WASM="${REPO_ROOT}/target/wasm32-unknown-unknown/release/ic_evm_wrapper.wasm"
OPT_WASM="${REPO_ROOT}/target/wasm32-unknown-unknown/release/ic_evm_wrapper.final.wasm"
PROFILED_WASM="${REPO_ROOT}/target/wasm32-unknown-unknown/release/ic_evm_wrapper.profiled.wasm"
START_PAGE="${START_PAGE:-}"
PAGE_LIMIT="${PAGE_LIMIT:-}"
SKIP_DEPLOY="${SKIP_DEPLOY:-0}"

log() {
  echo "[profile] $*"
}

build_release() {
  log "cargo build --release --target wasm32-unknown-unknown -p ic-evm-wrapper"
  cargo build --release --target wasm32-unknown-unknown -p ic-evm-wrapper
}

instrument_wasm() {
  if ! command -v ic-wasm >/dev/null 2>&1; then
    log "installing ic-wasm"
    cargo install ic-wasm --locked
  fi

  scripts/build_wasm_postprocess.sh "${INPUT_WASM}" "${OPT_WASM}"

  local -a cmd
  cmd=("ic-wasm" "${OPT_WASM}" "-o" "${PROFILED_WASM}" "instrument")

  IFS=',' read -r -a trace_items <<<"${TRACE_ONLY}"
  for fn in "${trace_items[@]}"; do
    local trimmed="${fn//[[:space:]]/}"
    if [[ -n "${trimmed}" ]]; then
      cmd+=("--trace-only" "${trimmed}")
    fi
  done

  if ic-wasm instrument --help | grep -q -- "--heap-trace"; then
    cmd+=("--heap-trace")
    log "instrument mode: heap-trace"
  else
    log "this ic-wasm has no --heap-trace; falling back to stable memory trace mode"
    if [[ -z "${START_PAGE}" ]]; then
      log "set START_PAGE to avoid stable memory overlap (example: START_PAGE=131072)"
      exit 1
    fi
    cmd+=("--start-page" "${START_PAGE}")
    if [[ -n "${PAGE_LIMIT}" ]]; then
      cmd+=("--page-limit" "${PAGE_LIMIT}")
    fi
  fi

  log "instrument wasm"
  if ! "${cmd[@]}"; then
    log "trace-only instrumentation failed; retry without --trace-only filters"
    local -a retry
    retry=("ic-wasm" "${OPT_WASM}" "-o" "${PROFILED_WASM}" "instrument")
    if ic-wasm instrument --help | grep -q -- "--heap-trace"; then
      retry+=("--heap-trace")
    else
      retry+=("--start-page" "${START_PAGE}")
      if [[ -n "${PAGE_LIMIT}" ]]; then
        retry+=("--page-limit" "${PAGE_LIMIT}")
      fi
    fi
    "${retry[@]}"
  fi
  log "instrumented wasm: ${PROFILED_WASM}"
  ic-wasm "${PROFILED_WASM}" info
}

deploy_profiled() {
  local dfx_cmd="dfx canister --network ${NETWORK}"
  local init_args
  init_args="$(build_init_args_for_current_identity 1000000000000000000)"

  if [[ "${NETWORK}" == "playground" || "${NETWORK}" == "ic" || "${NETWORK}" == "staging" ]]; then
    if [[ -z "${CANISTER_ID}" ]]; then
      log "CANISTER_ID is required for network=${NETWORK}"
      exit 1
    fi
    log "install profiled wasm to canister_id=${CANISTER_ID} mode=${MODE}"
    ${dfx_cmd} install --mode "${MODE}" --wasm "${PROFILED_WASM}" --argument "${init_args}" "${CANISTER_ID}"
  else
    log "install profiled wasm to canister_name=${CANISTER_NAME} mode=${MODE}"
    ${dfx_cmd} install --mode "${MODE}" --wasm "${PROFILED_WASM}" --argument "${init_args}" "${CANISTER_NAME}"
  fi

  cat <<'EOF'
[profile] next step examples:
  dfx canister --network "$NETWORK" call evm_canister set_auto_mine '(false)'
  dfx canister --network "$NETWORK" call evm_canister produce_block '(1:nat32)'
  dfx canister --network "$NETWORK" call evm_canister metrics '(60:nat64)'
EOF
}

build_release
instrument_wasm
if [[ "${SKIP_DEPLOY}" == "1" ]]; then
  log "skip deploy (SKIP_DEPLOY=1)"
else
  deploy_profiled
fi
