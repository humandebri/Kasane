#!/usr/bin/env bash
# where: mainnet post-upgrade smoke check
# what: verify minimum JSON-RPC behavior after canister upgrade
# why: detect regressions early before regular traffic hits the gateway
set -euo pipefail

RPC_URL="${EVM_RPC_URL:-https://rpc-testnet.kasane.network}"
CHECK_ADDR="${CHECK_ADDR:-0x0000000000000000000000000000000000000000}"
TEST_TX_HASH="${TEST_TX_HASH:-}"
CURL_TIMEOUT="${CURL_TIMEOUT:-20}"

log() {
  echo "[ic-post-smoke] $*"
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[ic-post-smoke] missing command: $1" >&2
    exit 1
  fi
}

rpc() {
  local method="$1"
  local params_json="$2"
  curl -fsS \
    --max-time "${CURL_TIMEOUT}" \
    -H 'content-type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"${method}\",\"params\":${params_json}}" \
    "${RPC_URL}"
}

assert_hex_result() {
  local response="$1"
  local method="$2"
  python - "${response}" "${method}" <<'PY'
import json
import re
import sys

response = json.loads(sys.argv[1])
method = sys.argv[2]
if "error" in response:
    raise SystemExit(f"[ic-post-smoke] {method} returned error: {response['error']}")
result = response.get("result")
if not isinstance(result, str) or re.fullmatch(r"0x[0-9a-fA-F]+", result) is None:
    raise SystemExit(f"[ic-post-smoke] {method} invalid result: {result!r}")
print(result)
PY
}

assert_data_result() {
  local response="$1"
  local method="$2"
  python - "${response}" "${method}" <<'PY'
import json
import re
import sys

response = json.loads(sys.argv[1])
method = sys.argv[2]
if "error" in response:
    raise SystemExit(f"[ic-post-smoke] {method} returned error: {response['error']}")
result = response.get("result")
if not isinstance(result, str) or re.fullmatch(r"0x[0-9a-fA-F]*", result) is None:
    raise SystemExit(f"[ic-post-smoke] {method} invalid result: {result!r}")
print(result)
PY
}

assert_getlogs_topic1_rejected() {
  local response="$1"
  python - "${response}" <<'PY'
import json
import sys

response = json.loads(sys.argv[1])
error = response.get("error")
if not isinstance(error, dict):
    raise SystemExit(f"[ic-post-smoke] eth_getLogs topic1 probe expected error, got: {response}")
code = error.get("code")
if code != -32602:
    raise SystemExit(f"[ic-post-smoke] eth_getLogs topic1 probe expected -32602, got: {code}")
print(code)
PY
}

assert_receipt_response_shape() {
  local response="$1"
  python - "${response}" <<'PY'
import json
import sys

response = json.loads(sys.argv[1])
if "error" in response:
    raise SystemExit(f"[ic-post-smoke] eth_getTransactionReceipt returned error: {response['error']}")
result = response.get("result")
if result is None:
    print("null")
    raise SystemExit(0)
if not isinstance(result, dict):
    raise SystemExit(f"[ic-post-smoke] eth_getTransactionReceipt invalid result: {result!r}")
status = result.get("status")
if status not in ("0x0", "0x1"):
    raise SystemExit(f"[ic-post-smoke] eth_getTransactionReceipt invalid status: {status!r}")
print(status)
PY
}

require_cmd curl
require_cmd python

log "rpc_url=${RPC_URL}"

log "eth_chainId"
CHAIN_ID_RESP="$(rpc "eth_chainId" "[]")"
CHAIN_ID_HEX="$(assert_hex_result "${CHAIN_ID_RESP}" "eth_chainId")"
log "eth_chainId=${CHAIN_ID_HEX}"

log "eth_blockNumber"
BLOCK_RESP="$(rpc "eth_blockNumber" "[]")"
BLOCK_HEX="$(assert_hex_result "${BLOCK_RESP}" "eth_blockNumber")"
log "eth_blockNumber=${BLOCK_HEX}"

log "eth_maxPriorityFeePerGas"
MAX_PRIORITY_RESP="$(rpc "eth_maxPriorityFeePerGas" "[]")"
MAX_PRIORITY_HEX="$(assert_hex_result "${MAX_PRIORITY_RESP}" "eth_maxPriorityFeePerGas")"
log "eth_maxPriorityFeePerGas=${MAX_PRIORITY_HEX}"

log "eth_getTransactionCount"
NONCE_RESP="$(rpc "eth_getTransactionCount" "[\"${CHECK_ADDR}\",\"latest\"]")"
NONCE_HEX="$(assert_hex_result "${NONCE_RESP}" "eth_getTransactionCount")"
log "eth_getTransactionCount=${NONCE_HEX} addr=${CHECK_ADDR}"

log "eth_getTransactionCount(head block number)"
NONCE_BY_HEAD_RESP="$(rpc "eth_getTransactionCount" "[\"${CHECK_ADDR}\",\"${BLOCK_HEX}\"]")"
NONCE_BY_HEAD_HEX="$(assert_hex_result "${NONCE_BY_HEAD_RESP}" "eth_getTransactionCount(head)")"
log "eth_getTransactionCount(head)=${NONCE_BY_HEAD_HEX} addr=${CHECK_ADDR} block=${BLOCK_HEX}"

log "eth_call(head block number)"
CALL_HEAD_RESP="$(rpc "eth_call" "[{\"to\":\"${CHECK_ADDR}\",\"data\":\"0x\"},\"${BLOCK_HEX}\"]")"
CALL_HEAD_DATA="$(assert_data_result "${CALL_HEAD_RESP}" "eth_call(head)")"
log "eth_call(head)=${CALL_HEAD_DATA} addr=${CHECK_ADDR} block=${BLOCK_HEX}"

log "eth_estimateGas(head block number)"
ESTIMATE_HEAD_RESP="$(rpc "eth_estimateGas" "[{\"from\":\"${CHECK_ADDR}\",\"to\":\"${CHECK_ADDR}\",\"value\":\"0x0\"},\"${BLOCK_HEX}\"]")"
ESTIMATE_HEAD_GAS="$(assert_hex_result "${ESTIMATE_HEAD_RESP}" "eth_estimateGas(head)")"
log "eth_estimateGas(head)=${ESTIMATE_HEAD_GAS} addr=${CHECK_ADDR} block=${BLOCK_HEX}"

log "eth_getLogs topic1 probe (expect -32602)"
GETLOGS_RESP="$(rpc "eth_getLogs" '[{"fromBlock":"latest","toBlock":"latest","topics":[null,"0x0000000000000000000000000000000000000000000000000000000000000000"]}]')"
GETLOGS_CODE="$(assert_getlogs_topic1_rejected "${GETLOGS_RESP}")"
log "eth_getLogs topic1 probe code=${GETLOGS_CODE}"

if [[ -n "${TEST_TX_HASH}" ]]; then
  log "eth_getTransactionReceipt tx=${TEST_TX_HASH}"
  RECEIPT_RESP="$(rpc "eth_getTransactionReceipt" "[\"${TEST_TX_HASH}\"]")"
  RECEIPT_STATUS="$(assert_receipt_response_shape "${RECEIPT_RESP}")"
  log "eth_getTransactionReceipt status=${RECEIPT_STATUS}"
fi

log "post-upgrade smoke passed"
