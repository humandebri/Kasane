#!/usr/bin/env bash
# where: mainnet operations testing
# what: run full-method canister test with cycle accounting and markdown report
# why: keep production verification repeatable, auditable, and reversible
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

source "${REPO_ROOT}/scripts/lib_candid_result.sh"
source "${REPO_ROOT}/scripts/mainnet/lib_mainnet_method_test.sh"

ICP_ENV="${ICP_ENV:-ic}"
CANISTER_ID="${CANISTER_ID:-4c52m-aiaaa-aaaam-agwwa-cai}"
ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME:-ci-local}"
IC_HOST="${IC_HOST:-https://icp-api.io}"
RUN_EXECUTE="${RUN_EXECUTE:-0}"
RUN_STRICT="${RUN_STRICT:-1}"
ICP_CALL_TIMEOUT_SEC="${ICP_CALL_TIMEOUT_SEC:-120}"
FULL_METHOD_REQUIRED="${FULL_METHOD_REQUIRED:-1}"
ALLOW_DESTRUCTIVE_PRUNE="${ALLOW_DESTRUCTIVE_PRUNE:-0}"
DRY_PRUNE_ONLY="${DRY_PRUNE_ONLY:-1}"
ETH_PRIVKEY="${ETH_PRIVKEY:-}"
AUTO_FUND_TEST_KEY="${AUTO_FUND_TEST_KEY:-1}"
AUTO_FUND_AMOUNT_WEI="${AUTO_FUND_AMOUNT_WEI:-500000000000000000}"
TX_MAX_FEE_PER_GAS_WEI="${TX_MAX_FEE_PER_GAS_WEI:-500000000000}"
TX_MAX_PRIORITY_FEE_PER_GAS_WEI="${TX_MAX_PRIORITY_FEE_PER_GAS_WEI:-250000000000}"
RAW_TX_GAS_PRICE_WEI="${RAW_TX_GAS_PRICE_WEI:-500000000000}"
TX_GAS_LIMIT="${TX_GAS_LIMIT:-21000}"
RUN_HEAVY_MATRIX="${RUN_HEAVY_MATRIX:-0}"
HEAVY_TX_PAYLOAD_BYTES_LIST="${HEAVY_TX_PAYLOAD_BYTES_LIST:-0,256,1024,4096}"
HEAVY_TX_REPEAT="${HEAVY_TX_REPEAT:-3}"
HEAVY_TX_GAS_LIMIT="${HEAVY_TX_GAS_LIMIT:-1500000}"
MINING_IDLE_OBSERVE_SEC="${MINING_IDLE_OBSERVE_SEC:-6}"
IDLE_MAX_CYCLE_DELTA="${IDLE_MAX_CYCLE_DELTA:-0}"
PRUNE_POLICY_TEST_ARGS="${PRUNE_POLICY_TEST_ARGS:-}"
PRUNE_POLICY_RESTORE_ARGS="${PRUNE_POLICY_RESTORE_ARGS:-}"
PRUNE_BLOCKS_ARGS="${PRUNE_BLOCKS_ARGS:-}"

# 推奨サンプル（84-block prune cadence向け）:
# PRUNE_POLICY_TEST_ARGS='(record {
#   headroom_ratio_bps = 2000:nat32;
#   target_bytes = 0:nat64;
#   retain_blocks = 168:nat64;
#   retain_days = 14:nat64;
#   hard_emergency_ratio_bps = 9500:nat32;
#   max_ops_per_tick = 300:nat32;
# })'
# PRUNE_POLICY_RESTORE_ARGS='(record {
#   headroom_ratio_bps = 2000:nat32;
#   target_bytes = 0:nat64;
#   retain_blocks = 168:nat64;
#   retain_days = 14:nat64;
#   hard_emergency_ratio_bps = 9500:nat32;
#   max_ops_per_tick = 300:nat32;
# })'
REPORT_DIR="${REPORT_DIR:-docs/ops/reports}"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
REPORT_FILE="${REPORT_DIR}/mainnet-method-test-${TIMESTAMP}.md"

INITIAL_BLOCK_GAS_LIMIT=""
INITIAL_INSTR_LIMIT=""
FINALIZE_DONE=0
EVENT_SEQ=0
CURRENT_STEP_ID=""
PROFILE_LABEL="safe"
if [[ "${FULL_METHOD_REQUIRED}" == "1" ]]; then
  PROFILE_LABEL="full"
fi

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[mainnet-test] missing command: $1" >&2
    exit 1
  fi
}

log() {
  echo "[mainnet-test] $*"
}

query_block_number() {
  local out
  out="$(run_icp_query_call "rpc_eth_block_number" "( )")"
  python - <<PY
import re
text = """${out}"""
m = re.search(r'([0-9][0-9_]*)\s*:\s*(?:nat|int)\d*', text)
if m:
    print(m.group(1).replace('_', ''))
else:
    candidates = []
    for token in re.finditer(r'(?<![0-9A-Za-z_])([0-9][0-9_]*)(?![0-9A-Za-z_])', text):
        raw = token.group(1).replace('_', '')
        if raw:
            candidates.append(int(raw))
    print(str(max(candidates)) if candidates else "0")
PY
}

observe_idle_mining_cycles() {
  local observe_sec="${1:-${MINING_IDLE_OBSERVE_SEC}}"
  local before_block after_block before_cycles after_cycles delta status note
  before_block="$(query_block_number)"
  before_cycles="$(get_cycles)"
  sleep "${observe_sec}"
  after_block="$(query_block_number)"
  after_cycles="$(get_cycles)"
  delta=$((before_cycles - after_cycles))
  status="ok"
  note="observe_sec=${observe_sec} start_block=${before_block} end_block=${after_block}"

  if [[ "${after_block}" -gt "${before_block}" ]]; then
    status="skipped:block_advanced"
    note="${note} reason=block_advanced"
  elif [[ "${IDLE_MAX_CYCLE_DELTA}" =~ ^[0-9]+$ ]] && [[ "${IDLE_MAX_CYCLE_DELTA}" -gt 0 ]] && [[ "${delta}" -gt "${IDLE_MAX_CYCLE_DELTA}" ]]; then
    status="err:idle_delta_exceeded"
    note="${note} idle_max_cycle_delta=${IDLE_MAX_CYCLE_DELTA}"
  fi

  record_cycle_row "mining_idle_cycle_probe" "-" "${status}" "${before_cycles}" "${after_cycles}" "${delta}" "${note}"
  record_method_row "mining_idle_cycle_probe" "event" "${status}" "delta=${delta} ${note}"
  if [[ "${status}" == "err:idle_delta_exceeded" && "${RUN_STRICT}" == "1" ]]; then
    return 125
  fi
}

wait_for_auto_production_block() {
  local note="${1:-auto-production settle}"
  local timeout_sec="${2:-60}"
  local before after delta start now
  before="$(get_cycles)"
  start="$(query_block_number)"
  now=0
  while (( now < timeout_sec )); do
    if [[ "$(query_block_number)" -gt "${start}" ]]; then
      break
    fi
    sleep 1
    now=$((now + 1))
  done
  after="$(get_cycles)"
  delta=$((before - after))
  if (( now >= timeout_sec )); then
    record_cycle_row "auto_production_wait" "-" "err:timeout" "${before}" "${after}" "${delta}" "${note} timeout=${timeout_sec}s"
    record_method_row "auto_production_wait" "event" "err:timeout" "${note} start_block=${start} waited=${now}s"
    if [[ "${RUN_STRICT}" == "1" ]]; then
      return 124
    fi
    return 0
  fi
  record_cycle_row "auto_production_wait" "-" "ok" "${before}" "${after}" "${delta}" "${note} timeout=${timeout_sec}s"
  record_method_row "auto_production_wait" "event" "ok" "${note} start_block=${start} waited=${now}s"
}

submit_ic_tx_with_retry_standard() {
  local note="$1"
  local max_attempts="${2:-3}"
  local attempt=1
  while (( attempt <= max_attempts )); do
    local nonce tx_args
    nonce="$(query_nonce_for_address "${CALLER_EVM_HEX}")"
    tx_args="$(generate_submit_ic_tx_bytes "${nonce}")"
    run_update_with_cycles "submit_ic_tx" "${tx_args}" "${note} attempt=${attempt} nonce=${nonce}" "1" >/dev/null
    if candid_is_ok "${RUN_UPDATE_LAST_OUT:-}" >/dev/null 2>&1; then
      return 0
    fi
    if printf '%s' "${RUN_UPDATE_LAST_OUT:-}" | grep -q "submit.tx_already_seen"; then
      local refreshed_nonce
      refreshed_nonce="$(query_nonce_for_address "${CALLER_EVM_HEX}")"
      if [[ "${refreshed_nonce}" -gt "${nonce}" ]]; then
        log "submit_ic_tx already_seen: nonce advanced (${nonce}->${refreshed_nonce}), 新nonceで再試行します"
      else
        # 方針: already_seen は「同一nonceの既存pending再利用」として扱い、失敗とはみなさない。
        log "submit_ic_tx already_seen: nonce unchanged (${nonce}), 既存pendingを再利用して続行します"
        return 0
      fi
      attempt=$((attempt + 1))
      continue
    fi
    if [[ "${RUN_STRICT}" == "1" ]]; then
      return 21
    fi
    return 0
  done

  if [[ "${RUN_STRICT}" == "1" ]]; then
    return 21
  fi
  return 0
}

submit_ic_tx_with_retry_custom() {
  local note="$1"
  local to_hex="$2"
  local value_wei="$3"
  local gas_limit="$4"
  local max_fee="$5"
  local max_priority="$6"
  local data_hex="${7-}"
  local max_attempts="${8:-3}"
  local attempt=1
  while (( attempt <= max_attempts )); do
    local nonce tx_args
    nonce="$(query_nonce_for_address "${CALLER_EVM_HEX}")"
    tx_args="$(generate_submit_ic_tx_bytes_custom "${nonce}" "${to_hex}" "${value_wei}" "${gas_limit}" "${max_fee}" "${max_priority}" "${data_hex}")"
    run_update_with_cycles "submit_ic_tx" "${tx_args}" "${note} attempt=${attempt} nonce=${nonce}" "1" >/dev/null
    if candid_is_ok "${RUN_UPDATE_LAST_OUT:-}" >/dev/null 2>&1; then
      return 0
    fi
    if printf '%s' "${RUN_UPDATE_LAST_OUT:-}" | grep -q "submit.tx_already_seen"; then
      local refreshed_nonce
      refreshed_nonce="$(query_nonce_for_address "${CALLER_EVM_HEX}")"
      if [[ "${refreshed_nonce}" -gt "${nonce}" ]]; then
        log "submit_ic_tx already_seen: nonce advanced (${nonce}->${refreshed_nonce}), 新nonceで再試行します"
      else
        # 方針: already_seen は「同一nonceの既存pending再利用」として扱い、失敗とはみなさない。
        log "submit_ic_tx already_seen: nonce unchanged (${nonce}), 既存pendingを再利用して続行します"
        return 0
      fi
      attempt=$((attempt + 1))
      continue
    fi
    if [[ "${RUN_STRICT}" == "1" ]]; then
      return 21
    fi
    return 0
  done

  if [[ "${RUN_STRICT}" == "1" ]]; then
    return 21
  fi
  return 0
}

assert_required() {
  local key="$1"
  local val="$2"
  if [[ -z "${val}" ]]; then
    echo "[mainnet-test] required env missing: ${key}" >&2
    exit 1
  fi
}

validate_execution_profile() {
  if [[ "${FULL_METHOD_REQUIRED}" != "1" ]]; then
    PROFILE_LABEL="safe"
    return
  fi
  PROFILE_LABEL="full"
  if [[ -z "${ETH_PRIVKEY}" && "${AUTO_FUND_TEST_KEY}" != "1" ]]; then
    echo "[mainnet-test] FULL_METHOD_REQUIRED=1 requires ETH_PRIVKEY or AUTO_FUND_TEST_KEY=1" >&2
    exit 1
  fi
  assert_required "PRUNE_POLICY_TEST_ARGS" "${PRUNE_POLICY_TEST_ARGS}"
  assert_required "PRUNE_POLICY_RESTORE_ARGS" "${PRUNE_POLICY_RESTORE_ARGS}"
  validate_candid_arg_text "PRUNE_POLICY_TEST_ARGS" "${PRUNE_POLICY_TEST_ARGS}"
  validate_candid_arg_text "PRUNE_POLICY_RESTORE_ARGS" "${PRUNE_POLICY_RESTORE_ARGS}"

  if [[ "${ALLOW_DESTRUCTIVE_PRUNE}" == "1" && "${DRY_PRUNE_ONLY}" != "1" ]]; then
    assert_required "PRUNE_BLOCKS_ARGS" "${PRUNE_BLOCKS_ARGS}"
    validate_candid_arg_text "PRUNE_BLOCKS_ARGS" "${PRUNE_BLOCKS_ARGS}"
  fi
}

finalize_state() {
  if [[ "${FINALIZE_DONE}" == "1" ]]; then
    return
  fi
  FINALIZE_DONE=1
  if [[ "${RUN_EXECUTE}" != "1" ]]; then
    return
  fi

  set +e
  run_update_with_cycles "set_pruning_enabled" "(false)" "finalize: enforce disabled" "1" >/dev/null
  if [[ -n "${INITIAL_BLOCK_GAS_LIMIT}" ]]; then
    run_update_with_cycles "set_block_gas_limit" "(${INITIAL_BLOCK_GAS_LIMIT}:nat64)" "finalize: restore gas" "1" >/dev/null
  fi
  if [[ -n "${INITIAL_INSTR_LIMIT}" ]]; then
    run_update_with_cycles "set_instruction_soft_limit" "(${INITIAL_INSTR_LIMIT}:nat64)" "finalize: restore instruction" "1" >/dev/null
  fi
  run_update_with_cycles "set_log_filter" "(null)" "finalize: clear log filter" "1" >/dev/null
  record_suite_event "suite_end" "finalize completed"
  set -e
}

require_cmd icp
require_cmd didc
require_cmd node
require_cmd shasum
require_cmd python
require_cmd cargo

mkdir -p "${REPORT_DIR}"
cat > "${REPORT_FILE}" <<EOF_REPORT
# Mainnet Method Test Report (${TIMESTAMP})

- canister: \`${CANISTER_ID}\`
- environment: \`${ICP_ENV}\`
- identity: \`${ICP_IDENTITY_NAME}\`
- execute: \`${RUN_EXECUTE}\`
- strict: \`${RUN_STRICT}\`
- full_method_required: \`${FULL_METHOD_REQUIRED}\`
- profile: \`${PROFILE_LABEL}\`
- auto_fund_test_key: \`${AUTO_FUND_TEST_KEY}\`
- auto_fund_amount_wei: \`${AUTO_FUND_AMOUNT_WEI}\`
- tx_max_fee_per_gas_wei: \`${TX_MAX_FEE_PER_GAS_WEI}\`
- tx_max_priority_fee_per_gas_wei: \`${TX_MAX_PRIORITY_FEE_PER_GAS_WEI}\`
- raw_tx_gas_price_wei: \`${RAW_TX_GAS_PRICE_WEI}\`
- tx_gas_limit: \`${TX_GAS_LIMIT}\`
- run_heavy_matrix: \`${RUN_HEAVY_MATRIX}\`
- heavy_tx_payload_bytes_list: \`${HEAVY_TX_PAYLOAD_BYTES_LIST}\`
- heavy_tx_repeat: \`${HEAVY_TX_REPEAT}\`
- heavy_tx_gas_limit: \`${HEAVY_TX_GAS_LIMIT}\`
- mining_idle_observe_sec: \`${MINING_IDLE_OBSERVE_SEC}\`
- idle_max_cycle_delta: \`${IDLE_MAX_CYCLE_DELTA}\`

## Cycle Events

| step_id | timestamp_utc | method | args_digest | status | before_cycles | after_cycles | delta | note |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |

## Method Results

| method | category | status | summary |
| --- | --- | --- | --- |

EOF_REPORT

if [[ "${RUN_EXECUTE}" != "1" ]]; then
  log "RUN_EXECUTE=0 dry-run only. script is implemented and ready."
  append_file ""
  append_file "## Dry Run"
  append_file "- RUN_EXECUTE=0 のため update 実行は未実施。"
  append_file "- 実行コマンド: \`RUN_EXECUTE=1 scripts/mainnet/mainnet_method_test.sh\`"
  echo "report=${REPORT_FILE}"
  exit 0
fi

validate_execution_profile

TMP_DID_RAW="$(mktemp -t evm_did.XXXXXX)"
TMP_DID_JS="${TMP_DID_RAW}.mjs"
QUERY_JSONL="$(mktemp -t evm_query_matrix.XXXXXX.jsonl)"
QUERY_SUMMARY="$(mktemp -t evm_query_summary.XXXXXX.json)"
QUERY_POST_JSONL="$(mktemp -t evm_query_post.XXXXXX.jsonl)"
HEAVY_METRICS_FILE="$(mktemp -t evm_heavy_metrics.XXXXXX.csv)"
trap 'finalize_state; rm -f "${TMP_DID_RAW:-}" "${TMP_DID_JS:-}" "${QUERY_JSONL:-}" "${QUERY_SUMMARY:-}" "${QUERY_POST_JSONL:-}" "${HEAVY_METRICS_FILE:-}"' EXIT

didc bind "${REPO_ROOT}/crates/ic-evm-wrapper/evm_canister.did" -t js > "${TMP_DID_JS}"
record_suite_event "suite_start" "execution start"

run_query_matrix "${QUERY_JSONL}" "${QUERY_SUMMARY}" "" "" ""
QUERY_OK_COUNT="$(awk -F'"ok":' '/"ok":/{if ($2 ~ /^true/) c++} END{print c+0}' "${QUERY_JSONL}")"
QUERY_TOTAL_COUNT="$(wc -l < "${QUERY_JSONL}" | tr -d ' ')"
record_method_row "query_matrix_baseline" "query" "ok=${QUERY_OK_COUNT}/${QUERY_TOTAL_COUNT}" "agent.query baseline completed"

INITIAL_BLOCK_GAS_LIMIT="$(node -e 'const fs=require("fs");const j=JSON.parse(fs.readFileSync(process.argv[1],"utf8"));console.log(j.get_ops_status?.block_gas_limit ?? "3000000");' "${QUERY_SUMMARY}")"
INITIAL_INSTR_LIMIT="$(node -e 'const fs=require("fs");const j=JSON.parse(fs.readFileSync(process.argv[1],"utf8"));console.log(j.get_ops_status?.instruction_soft_limit ?? "4000000000");' "${QUERY_SUMMARY}")"

run_update_with_cycles "set_pruning_enabled" "(false)" "baseline setup" "0" >/dev/null
observe_idle_mining_cycles "${MINING_IDLE_OBSERVE_SEC}"

CALLER_PRINCIPAL="$(icp identity principal --identity "${ICP_IDENTITY_NAME}")"
CALLER_EVM_HEX="$(cargo run -q -p ic-evm-core --bin derive_evm_address -- "${CALLER_PRINCIPAL}" | tr -d '\n\r')"
submit_ic_tx_with_retry_standard "write path A" "3"
SUBMIT_OUT="${RUN_UPDATE_LAST_OUT:-}"
IC_TX_ID_HEX=""
if candid_is_ok "${SUBMIT_OUT}" >/dev/null 2>&1; then
  IC_TX_ID_HEX="$(bytes_csv_to_hex "$(candid_extract_ok_blob_bytes "${SUBMIT_OUT}")")"
elif printf '%s' "${SUBMIT_OUT}" | grep -q "submit.tx_already_seen"; then
  record_method_row "submit_ic_tx_already_seen_policy" "policy" "accepted" "nonce unchanged の場合は既存pending再利用として成功扱い"
fi
wait_for_auto_production_block "write path A block production"

ETH_TX_ID_HEX=""
TEST_ETH_PRIVKEY="${ETH_PRIVKEY}"
AUTO_FUND_SKIP_REASON=""
if [[ -z "${TEST_ETH_PRIVKEY}" && "${AUTO_FUND_TEST_KEY}" == "1" ]]; then
  set +e
  CALLER_BALANCE_WEI="$(query_eth_balance_wei "${CALLER_EVM_HEX}")"
  BALANCE_RC=$?
  set -e
  if [[ "${BALANCE_RC}" -ne 0 || -z "${CALLER_BALANCE_WEI}" ]]; then
    if [[ "${RUN_STRICT}" == "1" ]]; then
      echo "[mainnet-test] auto fund skipped: failed to query caller balance" >&2
      exit 1
    fi
    AUTO_FUND_SKIP_REASON="caller_balance_query_failed"
    record_method_row "auto_fund_test_key" "update" "skipped" "${AUTO_FUND_SKIP_REASON}"
  else
    set +e
    FUND_AMOUNT_WEI="$(python - <<PY
balance = int("${CALLER_BALANCE_WEI}")
requested = int("${AUTO_FUND_AMOUNT_WEI}")
if requested <= 0:
    raise SystemExit(1)
cap = balance // 2
if cap <= 0:
    raise SystemExit(1)
print(requested if requested <= cap else cap)
PY
)"
    FUND_AMOUNT_RC=$?
    set -e
    if [[ "${FUND_AMOUNT_RC}" -ne 0 || -z "${FUND_AMOUNT_WEI}" ]]; then
      if [[ "${RUN_STRICT}" == "1" ]]; then
        echo "[mainnet-test] auto fund amount calculation failed" >&2
        exit 1
      fi
      AUTO_FUND_SKIP_REASON="auto_fund_amount_calculation_failed"
      record_method_row "auto_fund_test_key" "update" "skipped" "${AUTO_FUND_SKIP_REASON}"
    else
      TEST_ETH_PRIVKEY="$(cargo run -q -p ic-evm-core --features local-signer-bin --bin eth_raw_tx -- --mode genkey)"
      TEST_ETH_SENDER_HEX="$(eth_sender_hex_from_privkey "${TEST_ETH_PRIVKEY}" | tr -d '\n\r')"
      FUND_NONCE="$(query_nonce_for_address "${CALLER_EVM_HEX}")"
      FUND_TX_ARGS="$(generate_submit_ic_tx_bytes_custom "${FUND_NONCE}" "${TEST_ETH_SENDER_HEX}" "${FUND_AMOUNT_WEI}")"
      run_update_with_cycles "submit_ic_tx" "${FUND_TX_ARGS}" "auto-fund test ETH key ${TEST_ETH_SENDER_HEX}" "0" >/dev/null
      wait_for_auto_production_block "auto-fund block production"
      record_method_row "auto_fund_test_key" "update" "ok" "funded test sender=${TEST_ETH_SENDER_HEX} amount=${FUND_AMOUNT_WEI} caller_balance_before=${CALLER_BALANCE_WEI}"
    fi
  fi
fi

if [[ -n "${TEST_ETH_PRIVKEY}" ]]; then
  ETH_SENDER_HEX="$(eth_sender_hex_from_privkey "${TEST_ETH_PRIVKEY}" | tr -d '\n\r')"
  ETH_NONCE="$(query_nonce_for_address "${ETH_SENDER_HEX}")"
  ETH_RAW_TX_BYTES="$(build_eth_raw_tx_bytes "${ETH_NONCE}" "${TEST_ETH_PRIVKEY}")"
  set +e
  RAW_TX_DECODE_CHECK_OUT="$(validate_eth_raw_tx_bytes "${ETH_RAW_TX_BYTES}" "${TEST_ETH_PRIVKEY}" 2>&1)"
  RAW_TX_DECODE_RC=$?
  set -e
  RAW_TX_DECODE_STATUS="ok"
  if [[ "${RAW_TX_DECODE_RC}" -ne 0 ]]; then
    RAW_TX_DECODE_STATUS="err"
  fi
  record_method_row "raw_tx_local_decode_check" "local" "${RAW_TX_DECODE_STATUS}" "$(printf '%s' "${RAW_TX_DECODE_CHECK_OUT}" | tr '\n' ' ' | sed 's/|/\//g' | cut -c1-220)"
  if [[ "${RUN_STRICT}" == "1" && "${RAW_TX_DECODE_STATUS}" != "ok" ]]; then
    echo "[mainnet-test] local decode check failed for raw tx: ${RAW_TX_DECODE_CHECK_OUT}" >&2
    exit 1
  fi
  run_update_with_cycles "rpc_eth_send_raw_transaction" "(vec { ${ETH_RAW_TX_BYTES} })" "write path B" "0" >/dev/null
  SEND_OUT="${RUN_UPDATE_LAST_OUT:-}"
  if candid_is_ok "${SEND_OUT}" >/dev/null 2>&1; then
    ETH_TX_ID_HEX="$(bytes_csv_to_hex "$(candid_extract_ok_blob_bytes "${SEND_OUT}")")"
  fi
  RAW_TX_PENDING_STATUS_PRE="Unknown"
  RAW_TX_DROP_CODE_PRE=""
  RAW_TX_DROP_LABEL_PRE=""
  if [[ -n "${ETH_TX_ID_HEX}" ]]; then
    RAW_TX_PENDING_RAW_PRE="$(query_pending_for_tx_id "${ETH_TX_ID_HEX}")"
    RAW_TX_PENDING_STATUS_PRE="${RAW_TX_PENDING_RAW_PRE%%|*}"
    RAW_TX_DROP_CODE_PRE="${RAW_TX_PENDING_RAW_PRE#*|}"
    RAW_TX_DROP_LABEL_PRE="$(drop_code_label "${RAW_TX_DROP_CODE_PRE}")"
  fi
  RAW_TX_PRE_SUMMARY="tx_id=${ETH_TX_ID_HEX:-n/a} drop_code=${RAW_TX_DROP_CODE_PRE:-n/a} drop_label=${RAW_TX_DROP_LABEL_PRE:-n/a} local_decode=${RAW_TX_DECODE_STATUS:-n/a}"
  if [[ "${RAW_TX_PENDING_STATUS_PRE}" == "Dropped" && "${RAW_TX_DROP_CODE_PRE}" == "1" ]]; then
    RAW_TX_PRE_SUMMARY="${RAW_TX_PRE_SUMMARY} decode_probe=local_ok_canister_decode_drop"
  fi
  record_method_row "raw_tx_pending_status_pre" "query" "${RAW_TX_PENDING_STATUS_PRE}" "${RAW_TX_PRE_SUMMARY}"
  if [[ "${RUN_STRICT}" == "1" && "${RAW_TX_PENDING_STATUS_PRE}" == "Dropped" ]]; then
    echo "[mainnet-test] raw tx dropped before auto-production (drop_code=${RAW_TX_DROP_CODE_PRE:-n/a})" >&2
    exit 1
  fi

  wait_for_auto_production_block "write path B block production"
  RAW_TX_PENDING_STATUS_POST="Unknown"
  RAW_TX_DROP_CODE_POST=""
  RAW_TX_DROP_LABEL_POST=""
  if [[ -n "${ETH_TX_ID_HEX}" ]]; then
    RAW_TX_PENDING_RAW_POST="$(query_pending_for_tx_id "${ETH_TX_ID_HEX}")"
    RAW_TX_PENDING_STATUS_POST="${RAW_TX_PENDING_RAW_POST%%|*}"
    RAW_TX_DROP_CODE_POST="${RAW_TX_PENDING_RAW_POST#*|}"
    RAW_TX_DROP_LABEL_POST="$(drop_code_label "${RAW_TX_DROP_CODE_POST}")"
  fi
  RAW_TX_POST_SUMMARY="tx_id=${ETH_TX_ID_HEX:-n/a} drop_code=${RAW_TX_DROP_CODE_POST:-n/a} drop_label=${RAW_TX_DROP_LABEL_POST:-n/a} local_decode=${RAW_TX_DECODE_STATUS:-n/a}"
  if [[ "${RAW_TX_PENDING_STATUS_POST}" == "Dropped" && "${RAW_TX_DROP_CODE_POST}" == "1" ]]; then
    RAW_TX_POST_SUMMARY="${RAW_TX_POST_SUMMARY} decode_probe=local_ok_canister_decode_drop"
  fi
  record_method_row "raw_tx_pending_status_post" "query" "${RAW_TX_PENDING_STATUS_POST}" "${RAW_TX_POST_SUMMARY}"
  if [[ "${RUN_STRICT}" == "1" && "${RAW_TX_PENDING_STATUS_POST}" == "Dropped" ]]; then
    echo "[mainnet-test] raw tx dropped after auto-production (drop_code=${RAW_TX_DROP_CODE_POST:-n/a})" >&2
    exit 1
  fi
else
  record_method_row "rpc_eth_send_raw_transaction" "update" "skipped" "ETH_PRIVKEY 未指定か自動fund無効のため skip"
  record_method_row "raw_tx_pending_status_pre" "query" "skipped" "rpc_eth_send_raw_transaction skipped"
  record_method_row "raw_tx_pending_status_post" "query" "skipped" "rpc_eth_send_raw_transaction skipped"
fi

GAS_LIMITS=()
while IFS= read -r _gas_limit; do
  [[ -z "${_gas_limit}" ]] && continue
  GAS_LIMITS+=("${_gas_limit}")
done < <(python - <<PY
base = int("${INITIAL_BLOCK_GAS_LIMIT}")
factors = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0]
for f in factors:
    v = int(base * f)
    print(v if v >= 21000 else 21000)
PY
)
for limit in "${GAS_LIMITS[@]}"; do
  run_update_with_cycles "set_block_gas_limit" "(${limit}:nat64)" "gas sweep set" "0" >/dev/null
  submit_ic_tx_with_retry_standard "gas sweep submit limit=${limit}" "3"
  wait_for_auto_production_block "gas sweep produce limit=${limit}"
done
run_update_with_cycles "set_block_gas_limit" "(${INITIAL_BLOCK_GAS_LIMIT}:nat64)" "gas sweep restore" "0" >/dev/null

if [[ "${RUN_HEAVY_MATRIX}" == "1" ]]; then
  append_file ""
  append_file "## Heavy Tx Matrix"
  append_file "| payload_bytes | repeat | tx_id_hex | gas_used | submit_delta | produce_delta | pending_pre | pending_post |"
  append_file "| --- | --- | --- | --- | --- | --- | --- | --- |"
  IFS=',' read -r -a HEAVY_PAYLOADS <<< "${HEAVY_TX_PAYLOAD_BYTES_LIST// /}"
  for payload_bytes in "${HEAVY_PAYLOADS[@]}"; do
    [[ "${payload_bytes}" =~ ^[0-9]+$ ]] || continue
    for ((rep=1; rep<=HEAVY_TX_REPEAT; rep++)); do
      HEAVY_DATA_HEX="$(python - <<PY
n = int("${payload_bytes}")
print("11" * n)
PY
)"
      HEAVY_NONCE="$(query_nonce_for_address "${CALLER_EVM_HEX}")"
      HEAVY_CALLER_BALANCE_WEI="$(query_eth_balance_wei "${CALLER_EVM_HEX}")"
      HEAVY_REQUIRED_WEI="$(required_upfront_wei_for_tx "${HEAVY_TX_GAS_LIMIT}" "${TX_MAX_FEE_PER_GAS_WEI}" "0")"
      if [[ "${HEAVY_CALLER_BALANCE_WEI}" =~ ^[0-9]+$ && "${HEAVY_REQUIRED_WEI}" =~ ^[0-9]+$ ]]; then
        if python - <<PY
balance = int("${HEAVY_CALLER_BALANCE_WEI}")
required = int("${HEAVY_REQUIRED_WEI}")
raise SystemExit(0 if balance < required else 1)
PY
        then
          record_method_row "heavy_tx_case" "benchmark" "skipped_insufficient_balance" "payload_bytes=${payload_bytes} rep=${rep} balance=${HEAVY_CALLER_BALANCE_WEI} required=${HEAVY_REQUIRED_WEI}"
          append_file "| ${payload_bytes} | ${rep} | n/a | n/a | n/a | n/a | skipped | skipped |"
          if [[ "${RUN_STRICT}" == "1" ]]; then
            echo "[mainnet-test] heavy tx skipped due to insufficient balance (balance=${HEAVY_CALLER_BALANCE_WEI}, required=${HEAVY_REQUIRED_WEI})" >&2
            exit 1
          fi
          continue
        fi
      fi
      submit_ic_tx_with_retry_custom "heavy matrix submit payload=${payload_bytes} rep=${rep}" "0000000000000000000000000000000000000001" "0" "${HEAVY_TX_GAS_LIMIT}" "${TX_MAX_FEE_PER_GAS_WEI}" "${TX_MAX_PRIORITY_FEE_PER_GAS_WEI}" "${HEAVY_DATA_HEX}" "3"
      HEAVY_SUBMIT_DELTA="${RUN_UPDATE_LAST_DELTA:-n/a}"
      HEAVY_TX_ID_HEX=""
      HEAVY_SUBMIT_OUT="${RUN_UPDATE_LAST_OUT:-}"
      if candid_is_ok "${HEAVY_SUBMIT_OUT}" >/dev/null 2>&1; then
        HEAVY_TX_ID_HEX="$(bytes_csv_to_hex "$(candid_extract_ok_blob_bytes "${HEAVY_SUBMIT_OUT}")")"
      fi

      HEAVY_PENDING_PRE="Unknown"
      if [[ -n "${HEAVY_TX_ID_HEX}" ]]; then
        HEAVY_PENDING_PRE="$(query_pending_for_tx_id "${HEAVY_TX_ID_HEX}")"
      fi
      HEAVY_PENDING_PRE_STATUS="${HEAVY_PENDING_PRE%%|*}"

      wait_for_auto_production_block "heavy matrix produce payload=${payload_bytes} rep=${rep}"
      HEAVY_PRODUCE_DELTA="n/a"

      HEAVY_PENDING_POST="Unknown|"
      HEAVY_GAS_USED="n/a"
      if [[ -n "${HEAVY_TX_ID_HEX}" ]]; then
        HEAVY_PENDING_POST="$(query_pending_for_tx_id "${HEAVY_TX_ID_HEX}")"
        HEAVY_GAS_USED="$(query_receipt_gas_used_for_tx_id "${HEAVY_TX_ID_HEX}")"
      fi
      HEAVY_PENDING_POST_STATUS="${HEAVY_PENDING_POST%%|*}"

      record_method_row "heavy_tx_case" "benchmark" "ok" "payload_bytes=${payload_bytes} rep=${rep} tx_id=${HEAVY_TX_ID_HEX:-n/a} gas_used=${HEAVY_GAS_USED} pending_pre=${HEAVY_PENDING_PRE_STATUS} pending_post=${HEAVY_PENDING_POST_STATUS}"
      append_file "| ${payload_bytes} | ${rep} | ${HEAVY_TX_ID_HEX:-n/a} | ${HEAVY_GAS_USED} | ${HEAVY_SUBMIT_DELTA} | ${HEAVY_PRODUCE_DELTA} | ${HEAVY_PENDING_PRE_STATUS} | ${HEAVY_PENDING_POST_STATUS} |"

      if [[ "${HEAVY_GAS_USED}" =~ ^[0-9]+$ && "${HEAVY_SUBMIT_DELTA}" =~ ^[0-9]+$ && "${HEAVY_PRODUCE_DELTA}" =~ ^[0-9]+$ ]]; then
        printf '%s,%s,%s,%s,%s\n' "${payload_bytes}" "${rep}" "${HEAVY_GAS_USED}" "${HEAVY_SUBMIT_DELTA}" "${HEAVY_PRODUCE_DELTA}" >> "${HEAVY_METRICS_FILE}"
      fi
    done
  done

  if [[ -s "${HEAVY_METRICS_FILE}" ]]; then
    HEAVY_SUMMARY="$(python - <<PY
import csv, math
p = "${HEAVY_METRICS_FILE}"
rows = list(csv.reader(open(p)))
gas = sorted(int(r[2]) for r in rows)
cycles = sorted(int(r[3]) + int(r[4]) for r in rows)
def p95(vals):
    idx = max(0, min(len(vals)-1, math.ceil(0.95 * len(vals)) - 1))
    return vals[idx]
p95_gas = p95(gas)
p95_cycles = p95(cycles)
recommend = max(21000, int(p95_gas * 1.2))
print(f"{len(rows)}|{p95_gas}|{p95_cycles}|{recommend}")
PY
)"
    HEAVY_CASES="$(printf '%s' "${HEAVY_SUMMARY}" | cut -d'|' -f1)"
    HEAVY_P95_GAS="$(printf '%s' "${HEAVY_SUMMARY}" | cut -d'|' -f2)"
    HEAVY_P95_CYCLES="$(printf '%s' "${HEAVY_SUMMARY}" | cut -d'|' -f3)"
    HEAVY_RECOMMENDED_LIMIT="$(printf '%s' "${HEAVY_SUMMARY}" | cut -d'|' -f4)"
    append_file ""
    append_file "### Heavy Tx Summary"
    append_file "- cases: ${HEAVY_CASES}"
    append_file "- p95_gas_used: ${HEAVY_P95_GAS}"
    append_file "- p95_cycles_delta_total(submit+produce): ${HEAVY_P95_CYCLES}"
    append_file "- recommended_block_gas_limit(p95*1.2): ${HEAVY_RECOMMENDED_LIMIT}"
  else
    append_file ""
    append_file "### Heavy Tx Summary"
    append_file "- 有効ケースが得られなかったため推奨値は未算出。"
  fi
fi

TEST_INSTR_LIMIT="$(python - <<PY
v = int("${INITIAL_INSTR_LIMIT}")
t = int(v * 0.9)
print(t if t > 1000 else v)
PY
)"
run_update_with_cycles "set_instruction_soft_limit" "(${TEST_INSTR_LIMIT}:nat64)" "admin set" "0" >/dev/null
run_update_with_cycles "set_instruction_soft_limit" "(${INITIAL_INSTR_LIMIT}:nat64)" "admin restore" "0" >/dev/null
run_update_with_cycles "set_log_filter" "(opt \"info\")" "admin set" "0" >/dev/null
run_update_with_cycles "set_log_filter" "(null)" "admin restore" "0" >/dev/null

run_update_with_cycles "set_pruning_enabled" "(true)" "pruning visibility enable" "0" >/dev/null
if [[ -n "${PRUNE_POLICY_TEST_ARGS}" ]]; then
  run_update_with_cycles "set_prune_policy" "${PRUNE_POLICY_TEST_ARGS}" "prune policy set(test)" "0" >/dev/null
  if [[ -n "${PRUNE_POLICY_RESTORE_ARGS}" ]]; then
    run_update_with_cycles "set_prune_policy" "${PRUNE_POLICY_RESTORE_ARGS}" "prune policy restore" "0" >/dev/null
  else
    record_method_row "set_prune_policy" "update" "skipped_restore" "PRUNE_POLICY_RESTORE_ARGS 未指定"
  fi
else
  record_method_row "set_prune_policy" "update" "skipped" "PRUNE_POLICY_TEST_ARGS 未指定"
fi
if [[ "${ALLOW_DESTRUCTIVE_PRUNE}" == "1" && "${DRY_PRUNE_ONLY}" != "1" && -n "${PRUNE_BLOCKS_ARGS}" ]]; then
  run_update_with_cycles "prune_blocks" "${PRUNE_BLOCKS_ARGS}" "destructive prune opt-in" "0" >/dev/null
else
  record_method_row "prune_blocks" "update" "skipped" "本番データ保全のため destructive prune は未実行"
fi
run_update_with_cycles "set_pruning_enabled" "(false)" "pruning visibility disable" "0" >/dev/null

set +e
run_query_matrix "${QUERY_POST_JSONL}" "/dev/null" "${CALLER_EVM_HEX}" "${IC_TX_ID_HEX}" "${ETH_TX_ID_HEX}"
POST_QUERY_RC=$?
set -e
if [[ "${POST_QUERY_RC}" -ne 0 ]]; then
  if [[ "${RUN_STRICT}" == "1" ]]; then
    exit "${POST_QUERY_RC}"
  fi
  record_method_row "query_matrix_post" "query" "warn" "agent.query post checks failed (rc=${POST_QUERY_RC})"
else
  POST_QUERY_OK_COUNT="$(awk -F'"ok":' '/"ok":/{if ($2 ~ /^true/) c++} END{print c+0}' "${QUERY_POST_JSONL}")"
  POST_QUERY_TOTAL_COUNT="$(wc -l < "${QUERY_POST_JSONL}" | tr -d ' ')"
  record_method_row "query_matrix_post" "query" "ok=${POST_QUERY_OK_COUNT}/${POST_QUERY_TOTAL_COUNT}" "agent.query post checks completed"
fi

finalize_state
append_file ""
append_file "## Notes"
append_file "- tx_id_hex (submit_ic_tx): ${IC_TX_ID_HEX:-n/a}"
append_file "- tx_id_hex (rpc_eth_send_raw_transaction): ${ETH_TX_ID_HEX:-n/a}"
append_file "- test_eth_privkey_source: $( [[ -n "${ETH_PRIVKEY}" ]] && echo provided || ([[ -n "${TEST_ETH_PRIVKEY}" ]] && echo auto_funded || echo none) )"
append_file "- raw_tx_pending_status_pre: ${RAW_TX_PENDING_STATUS_PRE:-n/a}"
append_file "- raw_tx_drop_code_pre: ${RAW_TX_DROP_CODE_PRE:-n/a}"
append_file "- raw_tx_drop_label_pre: ${RAW_TX_DROP_LABEL_PRE:-n/a}"
append_file "- raw_tx_pending_status_post: ${RAW_TX_PENDING_STATUS_POST:-n/a}"
append_file "- raw_tx_drop_code_post: ${RAW_TX_DROP_CODE_POST:-n/a}"
append_file "- raw_tx_drop_label_post: ${RAW_TX_DROP_LABEL_POST:-n/a}"
append_file "- initial_block_gas_limit: ${INITIAL_BLOCK_GAS_LIMIT}"
append_file "- initial_instruction_soft_limit: ${INITIAL_INSTR_LIMIT}"
append_file "- report_generated_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"

echo "report=${REPORT_FILE}"
log "completed"
