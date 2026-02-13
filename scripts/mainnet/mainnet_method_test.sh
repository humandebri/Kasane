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
FULL_METHOD_REQUIRED="${FULL_METHOD_REQUIRED:-1}"
ALLOW_DESTRUCTIVE_PRUNE="${ALLOW_DESTRUCTIVE_PRUNE:-0}"
DRY_PRUNE_ONLY="${DRY_PRUNE_ONLY:-1}"
ETH_PRIVKEY="${ETH_PRIVKEY:-}"
PRUNE_POLICY_TEST_ARGS="${PRUNE_POLICY_TEST_ARGS:-}"
PRUNE_POLICY_RESTORE_ARGS="${PRUNE_POLICY_RESTORE_ARGS:-}"
PRUNE_BLOCKS_ARGS="${PRUNE_BLOCKS_ARGS:-}"
REPORT_DIR="${REPORT_DIR:-docs/ops/reports}"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
REPORT_FILE="${REPORT_DIR}/mainnet-method-test-${TIMESTAMP}.md"

INITIAL_BLOCK_GAS_LIMIT=""
INITIAL_INSTR_LIMIT=""
MINER_ALLOWLIST_ARGS="(vec {})"
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
  assert_required "ETH_PRIVKEY" "${ETH_PRIVKEY}"
  assert_required "PRUNE_POLICY_TEST_ARGS" "${PRUNE_POLICY_TEST_ARGS}"
  assert_required "PRUNE_POLICY_RESTORE_ARGS" "${PRUNE_POLICY_RESTORE_ARGS}"
  assert_required "PRUNE_BLOCKS_ARGS" "${PRUNE_BLOCKS_ARGS}"
  if [[ "${ALLOW_DESTRUCTIVE_PRUNE}" != "1" ]]; then
    echo "[mainnet-test] FULL_METHOD_REQUIRED=1 requires ALLOW_DESTRUCTIVE_PRUNE=1" >&2
    exit 1
  fi
  if [[ "${DRY_PRUNE_ONLY}" == "1" ]]; then
    echo "[mainnet-test] FULL_METHOD_REQUIRED=1 requires DRY_PRUNE_ONLY=0" >&2
    exit 1
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
  run_update_with_cycles "set_auto_mine" "(false)" "finalize: enforce disabled" "1" >/dev/null
  run_update_with_cycles "set_pruning_enabled" "(false)" "finalize: enforce disabled" "1" >/dev/null
  if [[ -n "${INITIAL_BLOCK_GAS_LIMIT}" ]]; then
    run_update_with_cycles "set_block_gas_limit" "(${INITIAL_BLOCK_GAS_LIMIT}:nat64)" "finalize: restore gas" "1" >/dev/null
  fi
  if [[ -n "${INITIAL_INSTR_LIMIT}" ]]; then
    run_update_with_cycles "set_instruction_soft_limit" "(${INITIAL_INSTR_LIMIT}:nat64)" "finalize: restore instruction" "1" >/dev/null
  fi
  run_update_with_cycles "set_log_filter" "(null)" "finalize: clear log filter" "1" >/dev/null
  run_update_with_cycles "set_miner_allowlist" "${MINER_ALLOWLIST_ARGS}" "finalize: restore allowlist" "1" >/dev/null
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
trap 'finalize_state; rm -f "${TMP_DID_RAW:-}" "${TMP_DID_JS:-}" "${QUERY_JSONL:-}" "${QUERY_SUMMARY:-}" "${QUERY_POST_JSONL:-}"' EXIT

didc bind "${REPO_ROOT}/crates/ic-evm-wrapper/evm_canister.did" -t js > "${TMP_DID_JS}"
record_suite_event "suite_start" "execution start"

run_query_matrix "${QUERY_JSONL}" "${QUERY_SUMMARY}" "" "" ""
QUERY_OK_COUNT="$(awk -F'"ok":' '/"ok":/{if ($2 ~ /^true/) c++} END{print c+0}' "${QUERY_JSONL}")"
QUERY_TOTAL_COUNT="$(wc -l < "${QUERY_JSONL}" | tr -d ' ')"
record_method_row "query_matrix_baseline" "query" "ok=${QUERY_OK_COUNT}/${QUERY_TOTAL_COUNT}" "agent.query baseline completed"

INITIAL_BLOCK_GAS_LIMIT="$(node -e 'const fs=require("fs");const j=JSON.parse(fs.readFileSync(process.argv[1],"utf8"));console.log(j.get_ops_status?.block_gas_limit ?? "3000000");' "${QUERY_SUMMARY}")"
INITIAL_INSTR_LIMIT="$(node -e 'const fs=require("fs");const j=JSON.parse(fs.readFileSync(process.argv[1],"utf8"));console.log(j.get_ops_status?.instruction_soft_limit ?? "4000000000");' "${QUERY_SUMMARY}")"
MINER_ALLOWLIST_JSON="$(node -e 'const fs=require("fs");const j=JSON.parse(fs.readFileSync(process.argv[1],"utf8"));console.log(JSON.stringify(j.get_miner_allowlist ?? []));' "${QUERY_SUMMARY}")"
MINER_ALLOWLIST_ARGS="$(node -e 'const arr=JSON.parse(process.argv[1]); if (!Array.isArray(arr) || arr.length===0){console.log("(vec {})"); process.exit(0);} console.log("(vec { " + arr.map((p)=>`principal \"${p}\"`).join("; ") + " })");' "${MINER_ALLOWLIST_JSON}")"

run_update_with_cycles "set_auto_mine" "(false)" "baseline setup" "0" >/dev/null
run_update_with_cycles "set_pruning_enabled" "(false)" "baseline setup" "0" >/dev/null

CALLER_PRINCIPAL="$(icp identity principal --identity "${ICP_IDENTITY_NAME}")"
CALLER_EVM_HEX="$(cargo run -q -p ic-evm-core --bin caller_evm -- "${CALLER_PRINCIPAL}" | tr -d '\n\r')"
CALLER_NONCE="$(query_nonce_for_address "${CALLER_EVM_HEX}")"
IC_TX_BYTES="$(generate_submit_ic_tx_bytes "${CALLER_NONCE}")"
SUBMIT_OUT="$(run_update_with_cycles "submit_ic_tx" "(vec { ${IC_TX_BYTES} })" "write path A" "0")"
IC_TX_ID_HEX=""
if candid_is_ok "${SUBMIT_OUT}" >/dev/null 2>&1; then
  IC_TX_ID_HEX="$(bytes_csv_to_hex "$(candid_extract_ok_blob_bytes "${SUBMIT_OUT}")")"
fi
run_update_with_cycles "produce_block" "(1:nat32)" "write path A block production" "0" >/dev/null

ETH_TX_ID_HEX=""
if [[ -n "${ETH_PRIVKEY}" ]]; then
  ETH_SENDER_HEX="$(eth_sender_hex_from_privkey "${ETH_PRIVKEY}" | tr -d '\n\r')"
  ETH_NONCE="$(query_nonce_for_address "${ETH_SENDER_HEX}")"
  ETH_RAW_TX_BYTES="$(build_eth_raw_tx_bytes "${ETH_NONCE}" "${ETH_PRIVKEY}")"
  SEND_OUT="$(run_update_with_cycles "rpc_eth_send_raw_transaction" "(vec { ${ETH_RAW_TX_BYTES} })" "write path B" "0")"
  if candid_is_ok "${SEND_OUT}" >/dev/null 2>&1; then
    ETH_TX_ID_HEX="$(bytes_csv_to_hex "$(candid_extract_ok_blob_bytes "${SEND_OUT}")")"
  fi
  run_update_with_cycles "produce_block" "(1:nat32)" "write path B block production" "0" >/dev/null
else
  record_method_row "rpc_eth_send_raw_transaction" "update" "skipped" "ETH_PRIVKEY 未指定のため skip"
fi

mapfile -t GAS_LIMITS < <(python - <<PY
base = int("${INITIAL_BLOCK_GAS_LIMIT}")
factors = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0]
for f in factors:
    v = int(base * f)
    print(v if v >= 21000 else 21000)
PY
)
for limit in "${GAS_LIMITS[@]}"; do
  run_update_with_cycles "set_block_gas_limit" "(${limit}:nat64)" "gas sweep set" "0" >/dev/null
  CALLER_NONCE="$(query_nonce_for_address "${CALLER_EVM_HEX}")"
  SWEEP_TX_BYTES="$(generate_submit_ic_tx_bytes "${CALLER_NONCE}")"
  run_update_with_cycles "submit_ic_tx" "(vec { ${SWEEP_TX_BYTES} })" "gas sweep submit limit=${limit}" "0" >/dev/null
  run_update_with_cycles "produce_block" "(1:nat32)" "gas sweep produce limit=${limit}" "0" >/dev/null
done
run_update_with_cycles "set_block_gas_limit" "(${INITIAL_BLOCK_GAS_LIMIT}:nat64)" "gas sweep restore" "0" >/dev/null

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
run_update_with_cycles "set_miner_allowlist" "${MINER_ALLOWLIST_ARGS}" "admin reapply" "0" >/dev/null

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

run_query_matrix "${QUERY_POST_JSONL}" "/dev/null" "${CALLER_EVM_HEX}" "${IC_TX_ID_HEX}" "${ETH_TX_ID_HEX}"
POST_QUERY_OK_COUNT="$(awk -F'"ok":' '/"ok":/{if ($2 ~ /^true/) c++} END{print c+0}' "${QUERY_POST_JSONL}")"
POST_QUERY_TOTAL_COUNT="$(wc -l < "${QUERY_POST_JSONL}" | tr -d ' ')"
record_method_row "query_matrix_post" "query" "ok=${POST_QUERY_OK_COUNT}/${POST_QUERY_TOTAL_COUNT}" "agent.query post checks completed"

finalize_state
append_file ""
append_file "## Notes"
append_file "- tx_id_hex (submit_ic_tx): ${IC_TX_ID_HEX:-n/a}"
append_file "- tx_id_hex (rpc_eth_send_raw_transaction): ${ETH_TX_ID_HEX:-n/a}"
append_file "- initial_block_gas_limit: ${INITIAL_BLOCK_GAS_LIMIT}"
append_file "- initial_instruction_soft_limit: ${INITIAL_INSTR_LIMIT}"
append_file "- report_generated_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"

echo "report=${REPORT_FILE}"
log "completed"
