#!/usr/bin/env bash
# where: mainnet test shared helper
# what: provide tx/candid utility functions for method test script
# why: keep the main script concise and readable

set -euo pipefail

generate_submit_ic_tx_bytes() {
  local nonce="$1"
  local max_fee="${TX_MAX_FEE_PER_GAS_WEI:-500000000000}"
  local max_priority="${TX_MAX_PRIORITY_FEE_PER_GAS_WEI:-250000000000}"
  local gas_limit="${TX_GAS_LIMIT:-21000}"
  python - <<PY
version = b'\\x02'
to = bytes.fromhex('0000000000000000000000000000000000000001')
value = (0).to_bytes(32, 'big')
gas = (int("${gas_limit}")).to_bytes(8, 'big')
nonce = (${nonce}).to_bytes(8, 'big')
max_fee = (int("${max_fee}")).to_bytes(16, 'big')
max_priority = (int("${max_priority}")).to_bytes(16, 'big')
data = b''
data_len = len(data).to_bytes(4, 'big')
tx = version + to + value + gas + nonce + max_fee + max_priority + data_len + data
print('; '.join(str(b) for b in tx))
PY
}

generate_submit_ic_tx_bytes_custom() {
  local nonce="$1"
  local to_hex="$2"
  local value_wei="$3"
  local gas_limit="${4:-500000}"
  local max_fee="${5:-${TX_MAX_FEE_PER_GAS_WEI:-500000000000}}"
  local max_priority="${6:-${TX_MAX_PRIORITY_FEE_PER_GAS_WEI:-250000000000}}"
  local data_hex="${7:-}"
  python - <<PY
version = b'\\x02'
to = bytes.fromhex("${to_hex}")
value = (int("${value_wei}")).to_bytes(32, 'big')
gas = (int("${gas_limit}")).to_bytes(8, 'big')
nonce = (int("${nonce}")).to_bytes(8, 'big')
max_fee = (int("${max_fee}")).to_bytes(16, 'big')
max_priority = (int("${max_priority}")).to_bytes(16, 'big')
data = bytes.fromhex("${data_hex}")
data_len = len(data).to_bytes(4, 'big')
tx = version + to + value + gas + nonce + max_fee + max_priority + data_len + data
print('; '.join(str(b) for b in tx))
PY
}

bytes_csv_to_hex() {
  local csv="$1"
  python - <<PY
items = [x.strip() for x in "${csv}".split(";") if x.strip()]
raw = bytes(int(x) for x in items)
print(raw.hex())
PY
}

build_eth_raw_tx_bytes() {
  local nonce="$1"
  local privkey="$2"
  local gas_price="${RAW_TX_GAS_PRICE_WEI:-500000000000}"
  cargo run -q -p ic-evm-core --features local-signer-bin --bin eth_raw_tx -- \
    --mode raw \
    --privkey "${privkey}" \
    --to "0000000000000000000000000000000000000001" \
    --value "1" \
    --gas-price "${gas_price}" \
    --gas-limit "21000" \
    --nonce "${nonce}" \
    --chain-id "4801360"
}

validate_eth_raw_tx_bytes() {
  local bytes_csv="$1"
  local privkey="$2"
  cargo run -q -p ic-evm-core --features local-signer-bin --bin eth_raw_tx -- \
    --mode decode-csv \
    --privkey "${privkey}" \
    --to "0000000000000000000000000000000000000001" \
    --value "0" \
    --gas-price "1" \
    --gas-limit "21000" \
    --nonce "0" \
    --chain-id "4801360" \
    --raw-csv "${bytes_csv}"
}

eth_sender_hex_from_privkey() {
  local privkey="$1"
  cargo run -q -p ic-evm-core --features local-signer-bin --bin eth_raw_tx -- \
    --mode sender-hex \
    --privkey "${privkey}" \
    --to "0000000000000000000000000000000000000001" \
    --value "0" \
    --gas-price "1" \
    --gas-limit "21000" \
    --nonce "0" \
    --chain-id "4801360"
}

run_icp_call() {
  local method="$1"
  local args="${2-}"
  local timeout_sec="${ICP_CALL_TIMEOUT_SEC:-120}"
  local cmd=(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" "${CANISTER_ID}" "${method}")
  if [[ -n "${args}" ]]; then
    cmd+=("${args}")
  fi
  if [[ "${timeout_sec}" -le 0 ]]; then
    "${cmd[@]}"
    return $?
  fi

  local out_file
  out_file="$(mktemp -t icp_call_out.XXXXXX)"
  "${cmd[@]}" >"${out_file}" 2>&1 &
  local pid=$!
  local elapsed=0
  while kill -0 "${pid}" >/dev/null 2>&1; do
    if [[ "${elapsed}" -ge "${timeout_sec}" ]]; then
      kill "${pid}" >/dev/null 2>&1 || true
      sleep 1
      kill -9 "${pid}" >/dev/null 2>&1 || true
      cat "${out_file}"
      rm -f "${out_file}"
      echo "[mainnet-test] icp call timeout: method=${method} timeout_sec=${timeout_sec}" >&2
      return 124
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done
  wait "${pid}"
  local rc=$?
  cat "${out_file}"
  rm -f "${out_file}"
  return "${rc}"
}

get_cycles() {
  local out
  out="$(icp canister status -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" "${CANISTER_ID}")"
  local cycles
  cycles="$(printf '%s\n' "${out}" | awk '/Cycles:/{v=$2; gsub(/_/, "", v); print v; exit}')"
  if [[ -z "${cycles}" ]]; then
    echo "[mainnet-test] failed to parse cycles from canister status" >&2
    exit 1
  fi
  echo "${cycles}"
}

sha_digest() {
  local input="${1-}"
  if [[ -z "${input}" ]]; then
    echo "-"
    return
  fi
  printf '%s' "${input}" | shasum -a 256 | awk '{print substr($1,1,12)}'
}

next_step_id() {
  CURRENT_STEP_ID="$(printf 'C%03d' "${EVENT_SEQ}")"
  EVENT_SEQ=$((EVENT_SEQ + 1))
}

append_file() {
  printf '%s\n' "$1" >> "${REPORT_FILE}"
}

record_cycle_row() {
  local method="$1"
  local args_digest="$2"
  local status="$3"
  local before="$4"
  local after="$5"
  local delta="$6"
  local note="$7"
  next_step_id
  append_file "| ${CURRENT_STEP_ID} | $(date -u +%Y-%m-%dT%H:%M:%SZ) | ${method} | ${args_digest} | ${status} | ${before} | ${after} | ${delta} | ${note} |"
}

record_method_row() {
  append_file "| $1 | $2 | $3 | $4 |"
}

record_suite_event() {
  local method="$1"
  local note="$2"
  local now_cycles
  now_cycles="$(get_cycles)"
  record_cycle_row "${method}" "-" "event" "${now_cycles}" "${now_cycles}" "0" "${note}"
}

run_update_with_cycles() {
  local method="$1"
  local args="${2-}"
  local note="${3-}"
  local allow_error="${4:-0}"
  local before after delta status out rc
  before="$(get_cycles)"
  set +e
  out="$(run_icp_call "${method}" "${args}" 2>&1)"
  rc=$?
  set -e
  after="$(get_cycles)"
  delta=$((before - after))

  if [[ "${rc}" -ne 0 ]]; then
    status="err:${rc}"
  elif candid_is_ok "${out}" >/dev/null 2>&1; then
    status="ok:variant_ok"
  else
    status="ok:variant_non_ok"
  fi

  record_cycle_row "${method}" "$(sha_digest "${args}")" "${status}" "${before}" "${after}" "${delta}" "${note}"
  record_method_row "${method}" "update" "${status}" "$(printf '%s' "${out}" | tr '\n' ' ' | sed 's/|/\//g' | cut -c1-220)"
  RUN_UPDATE_LAST_OUT="${out}"
  RUN_UPDATE_LAST_STATUS="${status}"
  RUN_UPDATE_LAST_DELTA="${delta}"
  printf '%s' "${out}"

  if [[ "${allow_error}" == "1" ]]; then
    return 0
  fi
  if [[ "${rc}" -ne 0 ]]; then
    if [[ "${RUN_STRICT}" == "1" ]]; then
      return "${rc}"
    fi
    return 0
  fi
  if [[ "${status}" == "ok:variant_non_ok" && "${RUN_STRICT}" == "1" ]]; then
    return 21
  fi
  return 0
}

run_query_matrix() {
  local out_file="$1"
  local summary_file="$2"
  local extra_address_hex="${3-}"
  local extra_tx_id_hex="${4-}"
  local extra_eth_hash_hex="${5-}"
  EVM_CANISTER_ID="${CANISTER_ID}" \
  INDEXER_IC_HOST="${IC_HOST}" \
  INDEXER_FETCH_ROOT_KEY="false" \
  EVM_DID_JS="${TMP_DID_JS}" \
  QUERY_OUT="${out_file}" \
  QUERY_SUMMARY_OUT="${summary_file}" \
  EXTRA_ADDRESS_HEX="${extra_address_hex}" \
  EXTRA_TX_ID_HEX="${extra_tx_id_hex}" \
  EXTRA_ETH_HASH_HEX="${extra_eth_hash_hex}" \
  node tools/indexer/src/mainnet_query_matrix.mjs
}

query_nonce_for_address() {
  local address_hex="$1"
  local nonce_jsonl
  nonce_jsonl="$(mktemp -t evm_query_nonce.XXXXXX.jsonl)"
  run_query_matrix "${nonce_jsonl}" "/dev/null" "${address_hex}" "" "" >/dev/null
  node -e 'const fs=require("fs");const p=process.argv[1];const addr=process.argv[2].toLowerCase();const lines=fs.readFileSync(p,"utf8").trim().split(/\n+/).filter(Boolean);for(const line of lines){const row=JSON.parse(line);if(row.method==="expected_nonce_by_address"&&row.ok&&row.address_hex===addr&&row.value&&row.value.Ok!==undefined){console.log(String(row.value.Ok));process.exit(0);}}console.log("0");' "${nonce_jsonl}" "${address_hex}"
  rm -f "${nonce_jsonl}"
}

hex_to_candid_blob_escape() {
  local hex="$1"
  python - <<PY
data = bytes.fromhex("${hex}")
print(''.join(f'\\\\{b:02x}' for b in data))
PY
}

csv_bytes_to_nat() {
  local csv="$1"
  python - <<PY
items = [x.strip() for x in "${csv}".split(";") if x.strip()]
raw = bytes(int(x) for x in items)
print(int.from_bytes(raw, "big"))
PY
}

query_eth_balance_wei() {
  local address_hex="$1"
  local balance_jsonl
  balance_jsonl="$(mktemp -t evm_query_balance.XXXXXX.jsonl)"
  run_query_matrix "${balance_jsonl}" "/dev/null" "${address_hex}" "" "" >/dev/null
  node -e 'const fs=require("fs");const p=process.argv[1];const addr=process.argv[2].toLowerCase();const lines=fs.readFileSync(p,"utf8").trim().split(/\n+/).filter(Boolean);for(const line of lines){const row=JSON.parse(line);if(row.method==="rpc_eth_get_balance"&&row.ok&&row.address_hex===addr&&row.value&&row.value.Ok&&typeof row.value.Ok.__hex==="string"){console.log(BigInt("0x"+row.value.Ok.__hex).toString());process.exit(0);}}process.exit(1);' "${balance_jsonl}" "${address_hex}"
  rm -f "${balance_jsonl}"
}

query_pending_for_tx_id() {
  local tx_id_hex="$1"
  local retry_count="${QUERY_RETRY_COUNT:-2}"
  local retry_sleep_sec="${QUERY_RETRY_SLEEP_SEC:-2}"
  local attempt=0
  while true; do
    local pending_jsonl
    pending_jsonl="$(mktemp -t evm_query_pending.XXXXXX.jsonl)"
    run_query_matrix "${pending_jsonl}" "/dev/null" "" "${tx_id_hex}" "" >/dev/null
    local result
    result="$(node -e 'const fs=require("fs");const p=process.argv[1];const tx=process.argv[2].toLowerCase();const lines=fs.readFileSync(p,"utf8").trim().split(/\n+/).filter(Boolean);for(const line of lines){const row=JSON.parse(line);if(row.method!=="get_pending"||!row.ok)continue;if((row.tx_id_hex||"").toLowerCase()!==tx)continue;const v=row.value||{};if(v.Included){console.log("Included|");process.exit(0);}if(v.Queued){console.log("Queued|");process.exit(0);}if(v.Dropped&&v.Dropped.code!==undefined){console.log(`Dropped|${String(v.Dropped.code)}`);process.exit(0);}if(v.Unknown!==undefined){console.log("Unknown|");process.exit(0);}}console.log("Unknown|");' "${pending_jsonl}" "${tx_id_hex}")"
    rm -f "${pending_jsonl}"
    if [[ "${result}" != "Unknown|" ]]; then
      echo "${result}"
      return 0
    fi
    if [[ "${attempt}" -ge "${retry_count}" ]]; then
      echo "${result}"
      return 0
    fi
    attempt=$((attempt + 1))
    sleep "${retry_sleep_sec}"
  done
}

drop_code_label() {
  local code="$1"
  case "${code}" in
    1) echo "decode" ;;
    2) echo "exec" ;;
    3) echo "missing" ;;
    4) echo "caller_missing" ;;
    5) echo "invalid_fee" ;;
    6) echo "replaced" ;;
    7) echo "result_too_large" ;;
    8) echo "block_gas_exceeded" ;;
    9) echo "instruction_budget" ;;
    10) echo "exec_precheck" ;;
    *) echo "unknown" ;;
  esac
}

required_upfront_wei_for_tx() {
  local gas_limit="$1"
  local max_fee_per_gas="$2"
  local value_wei="${3:-0}"
  python - <<PY
gas_limit = int("${gas_limit}")
max_fee = int("${max_fee_per_gas}")
value = int("${value_wei}")
print(gas_limit * max_fee + value)
PY
}

query_receipt_gas_used_for_tx_id() {
  local tx_id_hex="$1"
  local retry_count="${QUERY_RETRY_COUNT:-2}"
  local retry_sleep_sec="${QUERY_RETRY_SLEEP_SEC:-2}"
  local attempt=0
  while true; do
    local receipt_jsonl
    receipt_jsonl="$(mktemp -t evm_query_receipt.XXXXXX.jsonl)"
    run_query_matrix "${receipt_jsonl}" "/dev/null" "" "${tx_id_hex}" "" >/dev/null
    local gas
    gas="$(node -e 'const fs=require("fs");const p=process.argv[1];const tx=process.argv[2].toLowerCase();const lines=fs.readFileSync(p,"utf8").trim().split(/\n+/).filter(Boolean);for(const line of lines){const row=JSON.parse(line);if(row.method!=="get_receipt"||!row.ok)continue;if((row.tx_id_hex||"").toLowerCase()!==tx)continue;const v=row.value||{};const cands=[v.Ok&&v.Ok.gas_used,v.Found&&v.Found.gas_used,v.Found&&v.Found.receipt&&v.Found.receipt.gas_used,v.Receipt&&v.Receipt.gas_used];for(const one of cands){if(one!==undefined&&one!==null){console.log(String(one));process.exit(0);}}if(v.Err){console.log("n/a");process.exit(0);}}console.log("n/a");' "${receipt_jsonl}" "${tx_id_hex}")"
    rm -f "${receipt_jsonl}"
    if [[ "${gas}" != "n/a" ]]; then
      echo "${gas}"
      return 0
    fi
    if [[ "${attempt}" -ge "${retry_count}" ]]; then
      echo "${gas}"
      return 0
    fi
    attempt=$((attempt + 1))
    sleep "${retry_sleep_sec}"
  done
}

validate_candid_arg_text() {
  local label="$1"
  local candid_text="$2"
  if [[ -z "${candid_text}" ]]; then
    echo "[mainnet-test] ${label} is empty" >&2
    exit 1
  fi
  if ! didc encode "${candid_text}" >/dev/null 2>&1; then
    echo "[mainnet-test] ${label} is not valid candid text" >&2
    exit 1
  fi
}
