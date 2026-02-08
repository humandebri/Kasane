#!/usr/bin/env bash
# where: wasm post-processing helper shared by local CI and manual deploy
# what: run ic-wasm shrink/optimize/metadata/check-endpoints in one deterministic pipeline
# why: keep output wasm reproducible and prevent candid endpoint drift
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

INPUT_WASM="${1:-${REPO_ROOT}/target/wasm32-unknown-unknown/release/ic_evm_wrapper.wasm}"
OUTPUT_WASM="${2:-${REPO_ROOT}/target/wasm32-unknown-unknown/release/ic_evm_wrapper.final.wasm}"
DID_FILE="${DID_FILE:-${REPO_ROOT}/crates/ic-evm-wrapper/evm_canister.did}"
OPT_LEVEL="${OPT_LEVEL:-O3}"
ENABLE_STUB_WASI="${ENABLE_STUB_WASI:-0}"
CHECK_ENDPOINTS_EXCLUDE="${CHECK_ENDPOINTS_EXCLUDE:-rpc_eth_get_block_by_number_with_status,rpc_eth_get_transaction_receipt_with_status}"
CHECK_ENDPOINTS_HIDDEN="${CHECK_ENDPOINTS_HIDDEN:-canister_update:<ic-cdk internal> timer_executor,__getrandom_custom,canister_global_timer,canister_init,canister_inspect_message,canister_post_upgrade,canister_pre_upgrade,get_candid_pointer}"

if [[ ! -f "${INPUT_WASM}" ]]; then
  echo "[postprocess] input wasm not found: ${INPUT_WASM}" >&2
  exit 1
fi

if [[ ! -f "${DID_FILE}" ]]; then
  echo "[postprocess] did file not found: ${DID_FILE}" >&2
  exit 1
fi

if ! command -v ic-wasm >/dev/null 2>&1; then
  echo "[postprocess] installing ic-wasm"
  cargo install ic-wasm --locked
fi

WORK_DIR="$(dirname "${OUTPUT_WASM}")"
mkdir -p "${WORK_DIR}"

STUB_WASM="${WORK_DIR}/ic_evm_wrapper.stubbed.wasm"
SHRUNK_WASM="${WORK_DIR}/ic_evm_wrapper.shrunk.wasm"
OPT_WASM="${WORK_DIR}/ic_evm_wrapper.opt.wasm"

SOURCE_WASM="${INPUT_WASM}"

if [[ "${ENABLE_STUB_WASI}" == "1" ]]; then
  if ic-wasm shrink --help | grep -q -- "--stub-wasi"; then
    echo "[postprocess] stub wasi imports"
    ic-wasm "${SOURCE_WASM}" -o "${STUB_WASM}" shrink --stub-wasi
    SOURCE_WASM="${STUB_WASM}"
  else
    echo "[postprocess] ENABLE_STUB_WASI=1 was requested but this ic-wasm does not support --stub-wasi" >&2
    echo "[postprocess] installed: $(ic-wasm --version)" >&2
    exit 1
  fi
fi

echo "[postprocess] shrink"
ic-wasm "${SOURCE_WASM}" -o "${SHRUNK_WASM}" shrink

echo "[postprocess] optimize ${OPT_LEVEL}"
ic-wasm "${SHRUNK_WASM}" -o "${OPT_WASM}" optimize "${OPT_LEVEL}"

echo "[postprocess] metadata candid:service"
ic-wasm "${OPT_WASM}" -o "${OUTPUT_WASM}" metadata candid:service -f "${DID_FILE}" -v public

echo "[postprocess] check-endpoints"
CHECK_DID_FILE="${DID_FILE}"
if [[ -n "${CHECK_ENDPOINTS_EXCLUDE}" ]]; then
  CHECK_DID_FILE="$(mktemp -t ic_evm_wrapper.check.XXXXXX.did)"
  cp "${DID_FILE}" "${CHECK_DID_FILE}"
  IFS=',' read -r -a exclude_methods <<<"${CHECK_ENDPOINTS_EXCLUDE}"
  for method in "${exclude_methods[@]}"; do
    trimmed="${method//[[:space:]]/}"
    if [[ -n "${trimmed}" ]]; then
      perl -0pi -e "s/^\\s*${trimmed}\\s*:.*\\n//mg" "${CHECK_DID_FILE}"
    fi
  done
fi

check_cmd=("ic-wasm" "${OUTPUT_WASM}" "check-endpoints" "--candid" "${CHECK_DID_FILE}")
HIDDEN_FILE=""
if [[ -n "${CHECK_ENDPOINTS_HIDDEN}" ]]; then
  HIDDEN_FILE="$(mktemp -t ic_evm_wrapper.hidden.XXXXXX.txt)"
  IFS=',' read -r -a hidden_items <<<"${CHECK_ENDPOINTS_HIDDEN}"
  for item in "${hidden_items[@]}"; do
    trimmed="${item#"${item%%[![:space:]]*}"}"
    trimmed="${trimmed%"${trimmed##*[![:space:]]}"}"
    if [[ -n "${trimmed}" ]]; then
      printf '%s\n' "${trimmed}" >> "${HIDDEN_FILE}"
    fi
  done
  wasm_info="$(ic-wasm "${OUTPUT_WASM}" info)"
  if grep -q "canister_update dev_mint" <<<"${wasm_info}"; then
    printf '%s\n' "canister_update:dev_mint" >> "${HIDDEN_FILE}"
  fi
  if grep -q "canister_query __canbench__produce_block_path" <<<"${wasm_info}"; then
    printf '%s\n' "canister_query:__canbench__produce_block_path" >> "${HIDDEN_FILE}"
  fi
  if grep -q "canister_query __canbench__submit_ic_tx_path" <<<"${wasm_info}"; then
    printf '%s\n' "canister_query:__canbench__submit_ic_tx_path" >> "${HIDDEN_FILE}"
  fi
  if grep -q "canister_query __tracing__produce_block_path" <<<"${wasm_info}"; then
    printf '%s\n' "canister_query:__tracing__produce_block_path" >> "${HIDDEN_FILE}"
  fi
  if grep -q "canister_query __tracing__submit_ic_tx_path" <<<"${wasm_info}"; then
    printf '%s\n' "canister_query:__tracing__submit_ic_tx_path" >> "${HIDDEN_FILE}"
  fi
  if grep -q "\"__prepare_tracing" <<<"${wasm_info}"; then
    printf '%s\n' "__prepare_tracing" >> "${HIDDEN_FILE}"
  fi
  check_cmd+=("--hidden" "${HIDDEN_FILE}")
fi
"${check_cmd[@]}"

if [[ "${CHECK_DID_FILE}" != "${DID_FILE}" ]]; then
  rm -f "${CHECK_DID_FILE}"
fi
if [[ -n "${HIDDEN_FILE}" ]]; then
  rm -f "${HIDDEN_FILE}"
fi

echo "[postprocess] done: ${OUTPUT_WASM}"
ls -lh "${OUTPUT_WASM}"
