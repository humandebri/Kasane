#!/usr/bin/env bash
# where: mainnet test shared helper
# what: provide tx/candid utility functions for method test script
# why: keep the main script concise and readable

set -euo pipefail

generate_submit_ic_tx_bytes() {
  local nonce="$1"
  python - <<PY
version = b'\\x02'
to = bytes.fromhex('0000000000000000000000000000000000000001')
value = (0).to_bytes(32, 'big')
gas = (500000).to_bytes(8, 'big')
nonce = (${nonce}).to_bytes(8, 'big')
max_fee = (2_000_000_000).to_bytes(16, 'big')
max_priority = (1_000_000_000).to_bytes(16, 'big')
data = b''
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
  cargo run -q -p ic-evm-core --features local-signer-bin --bin eth_raw_tx -- \
    --mode raw \
    --privkey "${privkey}" \
    --to "0000000000000000000000000000000000000001" \
    --value "1" \
    --gas-price "1000000000" \
    --gas-limit "21000" \
    --nonce "${nonce}" \
    --chain-id "4801360"
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
  if [[ -n "${args}" ]]; then
    icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" "${CANISTER_ID}" "${method}" "${args}"
  else
    icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" "${CANISTER_ID}" "${method}"
  fi
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
