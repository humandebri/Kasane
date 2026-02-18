#!/usr/bin/env bash
# where: local pruning stage harness
# what: validate need_prune, gentle prune, aggressive prune (Pruned response)
# why: pruningは段階的に安全確認してから有効化するため
set -euo pipefail

NETWORK="${NETWORK:-local}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"

DFX_CANISTER="dfx canister --network ${NETWORK}"

log() {
  echo "[local-pruning-stage] $*"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "[local-pruning-stage] missing command: $1" >&2
    exit 1
  }
}

require_cmd dfx
require_cmd python

ensure_canister_ready() {
  if ! ${DFX_CANISTER} call "${CANISTER_NAME}" health --output json >/dev/null 2>&1; then
    echo "[local-pruning-stage] canister not ready. run scripts/local_indexer_smoke.sh first." >&2
    exit 1
  fi
}

build_ic_tx_hex() {
  local nonce="$1"
  python - <<PY
version = b'\\x02'
to = bytes.fromhex('0000000000000000000000000000000000000010')
value = (0).to_bytes(32, 'big')
gas = (500000).to_bytes(8, 'big')
nonce = (${nonce}).to_bytes(8, 'big')
max_fee = (2_000_000_000).to_bytes(16, 'big')
max_priority = (1_000_000_000).to_bytes(16, 'big')
data = b''
data_len = len(data).to_bytes(4, 'big')
tx = version + to + value + gas + nonce + max_fee + max_priority + data_len + data
print(tx.hex())
PY
}

hex_to_vec_bytes() {
  local hex="$1"
  python - <<PY
import sys
hex_str = "${hex}".strip()
raw = bytes.fromhex(hex_str)
print('; '.join(str(b) for b in raw))
PY
}

query_head_block() {
  local out
  out=$(${DFX_CANISTER} call --query "${CANISTER_NAME}" rpc_eth_block_number '( )' 2>/dev/null || true)
  python - <<PY
import re
text = """${out}"""
m = re.search(r'(\d+)', text)
print(m.group(1) if m else "0")
PY
}

wait_for_head_advance() {
  local start deadline
  start="$(query_head_block)"
  deadline=$((SECONDS + 30))
  while [[ ${SECONDS} -lt ${deadline} ]]; do
    if [[ "$(query_head_block)" -gt "${start}" ]]; then
      return 0
    fi
    sleep 1
  done
  echo "[local-pruning-stage] auto-mine did not advance head in time" >&2
  return 1
}

seed_blocks() {
  local start_nonce="$1"
  local count="$2"
  local i
  for i in $(seq 0 $((count - 1))); do
    local nonce=$((start_nonce + i))
    local tx_hex
    tx_hex=$(build_ic_tx_hex "${nonce}")
    local tx_bytes
    tx_bytes=$(hex_to_vec_bytes "${tx_hex}")
    ${DFX_CANISTER} call "${CANISTER_NAME}" submit_ic_tx "(vec { ${tx_bytes} })" >/dev/null
    wait_for_head_advance
  done
}

set_prune_policy() {
  local target_bytes="$1"
  local retain_blocks="$2"
  local retain_days="$3"
  local max_ops="$4"
  ${DFX_CANISTER} call "${CANISTER_NAME}" set_prune_policy "(record {
    headroom_ratio_bps = 2000;
    target_bytes = ${target_bytes}:nat64;
    retain_blocks = ${retain_blocks}:nat64;
    retain_days = ${retain_days}:nat64;
    hard_emergency_ratio_bps = 9500;
    max_ops_per_tick = ${max_ops}:nat32;
  })" >/dev/null
}

get_prune_status() {
  ${DFX_CANISTER} call "${CANISTER_NAME}" get_prune_status --output json
}

export_blocks() {
  local block_number="$1"
  ${DFX_CANISTER} call "${CANISTER_NAME}" export_blocks "(opt record { block_number = ${block_number}:nat64; segment = 0:nat8; byte_offset = 0:nat32 }, 1000000:nat32)" --output json
}

parse_need_prune() {
  python - <<PY
import json, sys
text = sys.stdin.read()
try:
  data = json.loads(text)
except Exception:
  sys.exit(1)
value = data.get("need_prune")
print("true" if value else "false")
PY
}

parse_pruned_before() {
  python - <<PY
import json, sys
text = sys.stdin.read()
try:
  data = json.loads(text)
except Exception:
  sys.exit(1)
value = data.get("pruned_before_block")
if value is None:
  print("")
else:
  print(value)
PY
}

ensure_canister_ready

log "seed blocks"
seed_blocks 10 6

log "stage 1: policy only (pruning disabled)"
${DFX_CANISTER} call "${CANISTER_NAME}" set_pruning_enabled '(false)' >/dev/null
set_prune_policy 1 2 1 200
need_prune=$(get_prune_status | parse_need_prune)
log "need_prune=${need_prune} (enabled=false)"

log "stage 2: gentle prune"
${DFX_CANISTER} call "${CANISTER_NAME}" set_pruning_enabled '(true)' >/dev/null
set_prune_policy 100000000 5 1 200
${DFX_CANISTER} call "${CANISTER_NAME}" prune_blocks '(5, 200)' >/dev/null
safe_export=$(export_blocks 12)
if echo "${safe_export}" | grep -q "Pruned"; then
  echo "[local-pruning-stage] unexpected Pruned during gentle policy" >&2
  exit 1
fi

log "stage 3: aggressive prune and Pruned response"
set_prune_policy 1 1 1 200
${DFX_CANISTER} call "${CANISTER_NAME}" prune_blocks '(1, 200)' >/dev/null
status=$(get_prune_status)
pruned_before=$(echo "${status}" | parse_pruned_before)
if [[ -z "${pruned_before}" ]]; then
  echo "[local-pruning-stage] pruned_before_block missing" >&2
  exit 1
fi
log "pruned_before_block=${pruned_before}"
pruned_export=$(export_blocks "${pruned_before}")
if ! echo "${pruned_export}" | grep -q "Pruned"; then
  echo "[local-pruning-stage] expected Pruned but got ok" >&2
  exit 1
fi

log "pruning stage finished"
