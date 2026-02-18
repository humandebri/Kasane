#!/usr/bin/env bash
# where: local pruning stage harness
# what: validate need_prune, gentle prune, aggressive prune (Pruned response)
# why: pruningは段階的に安全確認してから有効化するため
set -euo pipefail
source "$(dirname "$0")/lib_candid_result.sh"

NETWORK="${NETWORK:-local}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME:-ci-local}"
SEED_TX_MAX_FEE_WEI="${SEED_TX_MAX_FEE_WEI:-1000000000000}"
SEED_TX_MAX_PRIORITY_FEE_WEI="${SEED_TX_MAX_PRIORITY_FEE_WEI:-250000000000}"
SEED_SUBMIT_TIMEOUT_SEC="${SEED_SUBMIT_TIMEOUT_SEC:-20}"
SEED_BLOCK_COUNT="${SEED_BLOCK_COUNT:-0}"

ICP_CANISTER_CALL=(icp canister call -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}")

log() {
  echo "[local-pruning-stage] $*"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "[local-pruning-stage] missing command: $1" >&2
    exit 1
  }
}

require_cmd icp
require_cmd python
require_cmd npm

submit_ic_tx_with_timeout() {
  local tx_bytes="$1"
  NETWORK="${NETWORK}" \
  ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME}" \
  CANISTER_NAME="${CANISTER_NAME}" \
  TX_BYTES="${tx_bytes}" \
  SEED_SUBMIT_TIMEOUT_SEC="${SEED_SUBMIT_TIMEOUT_SEC}" \
  python - <<'PY'
import os
import subprocess
import sys

cmd = [
    "icp",
    "canister",
    "call",
    "-e",
    os.environ["NETWORK"],
    "--identity",
    os.environ["ICP_IDENTITY_NAME"],
    os.environ["CANISTER_NAME"],
    "submit_ic_tx",
    f"(vec {{ {os.environ['TX_BYTES']} }})",
]
timeout = int(os.environ["SEED_SUBMIT_TIMEOUT_SEC"])
try:
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    out = (result.stdout or "") + (result.stderr or "")
    print(out.strip())
    sys.exit(0)
except subprocess.TimeoutExpired as err:
    out = (err.stdout or "") + (err.stderr or "")
    if isinstance(out, bytes):
      out = out.decode(errors="ignore")
    if out:
      print(out.strip())
    print("submit call timed out")
    sys.exit(124)
PY
}

replica_api_host() {
  local status_json
  status_json=$(icp network status "${NETWORK}" --json 2>/dev/null || true)
  API_STATUS_JSON="${status_json}" python - <<'PY'
import json
import os
text = os.environ.get("API_STATUS_JSON", "").strip()
if not text:
    print("http://127.0.0.1:4943")
    raise SystemExit(0)
try:
    data = json.loads(text)
except Exception:
    print("http://127.0.0.1:4943")
    raise SystemExit(0)
port = data.get("port")
if isinstance(port, int) and port > 0:
    print(f"http://127.0.0.1:{port}")
else:
    print("http://127.0.0.1:4943")
PY
}

resolve_canister_id() {
  icp canister status -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --id-only "${CANISTER_NAME}"
}

ensure_indexer_node_modules() {
  if [[ ! -d tools/indexer/node_modules ]]; then
    echo "[local-pruning-stage] npm install (tools/indexer)"
    (cd tools/indexer && npm install)
  fi
}

ensure_canister_ready() {
  if ! "${ICP_CANISTER_CALL[@]}" "${CANISTER_NAME}" health >/dev/null 2>&1; then
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
max_fee = (${SEED_TX_MAX_FEE_WEI}).to_bytes(16, 'big')
max_priority = (${SEED_TX_MAX_PRIORITY_FEE_WEI}).to_bytes(16, 'big')
try:
    import time
    data = int(time.time_ns()).to_bytes(8, 'big')
except Exception:
    data = b'\x01'
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
  local canister_id
  local host
  canister_id="$(resolve_canister_id)"
  host="$(replica_api_host)"
  (
    cd tools/indexer
    EVM_CANISTER_ID="${canister_id}" \
    INDEXER_IC_HOST="${host}" \
    INDEXER_FETCH_ROOT_KEY="true" \
    ./node_modules/.bin/tsx <<'TS'
import { Actor, HttpAgent } from "@dfinity/agent";
process.stdout.on("error", (err: any) => {
  if (err?.code === "EPIPE") process.exit(0);
  throw err;
});

const canisterId = process.env.EVM_CANISTER_ID;
const host = process.env.INDEXER_IC_HOST ?? "http://127.0.0.1:4943";
const fetchRootKey = process.env.INDEXER_FETCH_ROOT_KEY === "true";
if (!canisterId) throw new Error("missing EVM_CANISTER_ID");
const idlFactory = ({ IDL }) =>
  IDL.Service({
    rpc_eth_block_number: IDL.Func([], [IDL.Nat64], ["query"]),
  });
const agent = new HttpAgent({ host, fetch: globalThis.fetch });
if (fetchRootKey) {
  await agent.fetchRootKey();
}
const actor = Actor.createActor(idlFactory as any, { agent, canisterId }) as any;
const head = await actor.rpc_eth_block_number();
const value = typeof head === "bigint" ? head : BigInt(head);
console.log(value.toString());
TS
  )
}

query_expected_nonce() {
  local principal
  local address_hex
  local canister_id
  local host
  principal="$(icp identity principal --identity "${ICP_IDENTITY_NAME}")"
  address_hex="$(cargo run -q -p ic-evm-core --bin derive_evm_address -- "${principal}")"
  canister_id="$(resolve_canister_id)"
  host="$(replica_api_host)"
  (
    cd tools/indexer
    EVM_CANISTER_ID="${canister_id}" \
    INDEXER_IC_HOST="${host}" \
    INDEXER_FETCH_ROOT_KEY="true" \
    ADDRESS_HEX="${address_hex}" \
    ./node_modules/.bin/tsx <<'TS'
import { Actor, HttpAgent } from "@dfinity/agent";

const canisterId = process.env.EVM_CANISTER_ID;
const host = process.env.INDEXER_IC_HOST ?? "http://127.0.0.1:4943";
const fetchRootKey = process.env.INDEXER_FETCH_ROOT_KEY === "true";
const addressHex = process.env.ADDRESS_HEX ?? "";
if (!canisterId) throw new Error("missing EVM_CANISTER_ID");
const idlFactory = ({ IDL }) =>
  IDL.Service({
    expected_nonce_by_address: IDL.Func(
      [IDL.Vec(IDL.Nat8)],
      [IDL.Variant({ Ok: IDL.Nat64, Err: IDL.Text })],
      ["query"]
    ),
  });
const bytes = Uint8Array.from((addressHex.match(/.{1,2}/g) ?? []).map((b) => parseInt(b, 16)));
const agent = new HttpAgent({ host, fetch: globalThis.fetch });
if (fetchRootKey) {
  await agent.fetchRootKey();
}
const actor = Actor.createActor(idlFactory as any, { agent, canisterId }) as any;
const result = await actor.expected_nonce_by_address(bytes);
if ("Err" in result) {
  throw new Error(String(result.Err));
}
const value = typeof result.Ok === "bigint" ? result.Ok : BigInt(result.Ok);
console.log(value.toString());
TS
  )
}

wait_for_head_advance() {
  local start="$1"
  local deadline current target
  target=$((start + 1))
  deadline=$((SECONDS + 30))
  while [[ ${SECONDS} -lt ${deadline} ]]; do
    current="$(query_head_block)"
    if (( current >= target )); then
      return 0
    fi
    sleep 1
  done
  current="$(query_head_block)"
  echo "[local-pruning-stage] auto-mine did not advance head in time (start=${start} target=${target} current=${current})" >&2
  return 1
}

seed_blocks() {
  local count="$1"
  local i
  for i in $(seq 0 $((count - 1))); do
    local next_nonce
    local start_head
    start_head="$(query_head_block)"
    next_nonce="$(query_expected_nonce)"
    log "seed iteration=$i start_head=${start_head}"
    local accepted=0
    local attempts=0
    while [[ "${accepted}" -ne 1 && "${attempts}" -lt 128 ]]; do
      local tx_hex
      tx_hex=$(build_ic_tx_hex "${next_nonce}")
      local tx_bytes
      tx_bytes=$(hex_to_vec_bytes "${tx_hex}")
      local out
      out="$(submit_ic_tx_with_timeout "${tx_bytes}" || true)"
      if grep -qi "timed out" <<<"${out}" || [[ -z "${out}" ]]; then
        attempts=$((attempts + 1))
        if (( attempts % 16 == 0 )); then
          log "seed retry timeout attempts=${attempts} nonce=${next_nonce}"
        fi
        continue
      fi
      if candid_is_ok "${out}"; then
        log "seed accepted nonce=${next_nonce} out=${out}"
        accepted=1
        break
      fi
      if grep -qi 'nonce too low\|tx_already_seen' <<<"${out}"; then
        next_nonce=$((next_nonce + 1))
        attempts=$((attempts + 1))
        if (( attempts % 16 == 0 )); then
          log "seed retry low_or_seen attempts=${attempts} nonce=${next_nonce}"
        fi
        continue
      fi
      if grep -qi 'nonce_gap' <<<"${out}"; then
        if [[ "${next_nonce}" -gt 0 ]]; then
          next_nonce=$((next_nonce - 1))
        fi
        attempts=$((attempts + 1))
        if (( attempts % 16 == 0 )); then
          log "seed retry nonce_gap attempts=${attempts} nonce=${next_nonce}"
        fi
        continue
      fi
      echo "[local-pruning-stage] seed submit failed: ${out}" >&2
      return 1
    done
    if [[ "${accepted}" -ne 1 ]]; then
      echo "[local-pruning-stage] failed to submit seed tx after retries" >&2
      return 1
    fi
    wait_for_head_advance "${start_head}"
  done
}

set_prune_policy() {
  local target_bytes="$1"
  local retain_blocks="$2"
  local retain_days="$3"
  local max_ops="$4"
  "${ICP_CANISTER_CALL[@]}" "${CANISTER_NAME}" set_prune_policy "(record {
    headroom_ratio_bps = 2000;
    target_bytes = ${target_bytes}:nat64;
    retain_blocks = ${retain_blocks}:nat64;
    retain_days = ${retain_days}:nat64;
    hard_emergency_ratio_bps = 9500;
    max_ops_per_tick = ${max_ops}:nat32;
  })" >/dev/null
}

get_prune_status() {
  local canister_id
  local host
  canister_id="$(resolve_canister_id)"
  host="$(replica_api_host)"
  (
    cd tools/indexer
    EVM_CANISTER_ID="${canister_id}" \
    INDEXER_IC_HOST="${host}" \
    INDEXER_FETCH_ROOT_KEY="true" \
    ./node_modules/.bin/tsx <<'TS'
import { Actor, HttpAgent } from "@dfinity/agent";

const canisterId = process.env.EVM_CANISTER_ID;
const host = process.env.INDEXER_IC_HOST ?? "http://127.0.0.1:4943";
const fetchRootKey = process.env.INDEXER_FETCH_ROOT_KEY === "true";
if (!canisterId) {
  throw new Error("missing EVM_CANISTER_ID");
}
const agent = new HttpAgent({ host, fetch: globalThis.fetch });
if (fetchRootKey) {
  await agent.fetchRootKey();
}
const idlFactory = ({ IDL }) =>
  IDL.Service({
    get_prune_status: IDL.Func(
      [],
      [
        IDL.Record({
          pruning_enabled: IDL.Bool,
          prune_running: IDL.Bool,
          estimated_kept_bytes: IDL.Nat64,
          high_water_bytes: IDL.Nat64,
          low_water_bytes: IDL.Nat64,
          hard_emergency_bytes: IDL.Nat64,
          last_prune_at: IDL.Nat64,
          pruned_before_block: IDL.Opt(IDL.Nat64),
          oldest_kept_block: IDL.Opt(IDL.Nat64),
          oldest_kept_timestamp: IDL.Opt(IDL.Nat64),
          need_prune: IDL.Bool,
        }),
      ],
      ["query"]
    ),
  });
const actor = Actor.createActor(idlFactory as any, { agent, canisterId }) as any;
const status = await actor.get_prune_status();
const opt = (value) => {
  if (Array.isArray(value)) {
    return value.length > 0 ? value[0] : null;
  }
  return value ?? null;
};
const prunedBefore = opt(status.pruned_before_block);
console.log(
  JSON.stringify({
    need_prune: Boolean(status.need_prune),
    pruned_before_block: prunedBefore === null ? null : prunedBefore.toString(),
  })
);
TS
  )
}

export_blocks() {
  local block_number="$1"
  local canister_id
  local host
  canister_id="$(resolve_canister_id)"
  host="$(replica_api_host)"
  (
    cd tools/indexer
    EVM_CANISTER_ID="${canister_id}" \
    INDEXER_IC_HOST="${host}" \
    INDEXER_FETCH_ROOT_KEY="true" \
    BLOCK_NUMBER="${block_number}" \
    ./node_modules/.bin/tsx <<'TS'
import { Actor, HttpAgent } from "@dfinity/agent";
process.stdout.on("error", (err: any) => {
  if (err?.code === "EPIPE") process.exit(0);
  throw err;
});

const canisterId = process.env.EVM_CANISTER_ID;
const host = process.env.INDEXER_IC_HOST ?? "http://127.0.0.1:4943";
const fetchRootKey = process.env.INDEXER_FETCH_ROOT_KEY === "true";
const blockNumber = BigInt(process.env.BLOCK_NUMBER ?? "0");
if (!canisterId) {
  throw new Error("missing EVM_CANISTER_ID");
}
const agent = new HttpAgent({ host, fetch: globalThis.fetch });
if (fetchRootKey) {
  await agent.fetchRootKey();
}
const idlFactory = ({ IDL }) => {
  const Cursor = IDL.Record({
    block_number: IDL.Nat64,
    segment: IDL.Nat8,
    byte_offset: IDL.Nat32,
  });
  const Chunk = IDL.Record({
    segment: IDL.Nat8,
    start: IDL.Nat32,
    bytes: IDL.Vec(IDL.Nat8),
    payload_len: IDL.Nat32,
  });
  const ExportResponse = IDL.Record({
    chunks: IDL.Vec(Chunk),
    next_cursor: IDL.Opt(Cursor),
  });
  const ExportError = IDL.Variant({
    InvalidCursor: IDL.Record({ message: IDL.Text }),
    Pruned: IDL.Record({ pruned_before_block: IDL.Nat64 }),
    MissingData: IDL.Record({ message: IDL.Text }),
    Limit: IDL.Null,
  });
  return IDL.Service({
    export_blocks: IDL.Func([IDL.Opt(Cursor), IDL.Nat32], [IDL.Variant({ Ok: ExportResponse, Err: ExportError })], ["query"]),
  });
};
const actor = Actor.createActor(idlFactory as any, { agent, canisterId }) as any;
const cursor = [{ block_number: blockNumber, segment: 0, byte_offset: 0 }];
const out = await actor.export_blocks(cursor, 1_000_000);
const normalize = (value) => {
  if (typeof value === "bigint") {
    return value.toString();
  }
  if (Array.isArray(value)) {
    return value.map(normalize);
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, normalize(v)]));
  }
  return value;
};
console.log(JSON.stringify(normalize(out)));
TS
  )
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
ensure_indexer_node_modules

if [[ "${SEED_BLOCK_COUNT}" -gt 0 ]]; then
  log "seed blocks (count=${SEED_BLOCK_COUNT})"
  seed_blocks "${SEED_BLOCK_COUNT}"
else
  log "seed blocks skipped (SEED_BLOCK_COUNT=0)"
fi

log "stage 1: policy only (pruning disabled)"
"${ICP_CANISTER_CALL[@]}" "${CANISTER_NAME}" set_pruning_enabled '(false)' >/dev/null
set_prune_policy 1 2 1 200
status_stage1="$(get_prune_status)"
need_prune="$(
  STATUS_JSON="${status_stage1}" python - <<'PY'
import json
import os
text = os.environ.get("STATUS_JSON", "")
data = json.loads(text)
print("true" if data.get("need_prune") else "false")
PY
)"
log "need_prune=${need_prune} (enabled=false)"

log "stage 2: gentle prune"
"${ICP_CANISTER_CALL[@]}" "${CANISTER_NAME}" set_pruning_enabled '(true)' >/dev/null
set_prune_policy 100000000 5 1 200
"${ICP_CANISTER_CALL[@]}" "${CANISTER_NAME}" prune_blocks '(5, 200)' >/dev/null
safe_export=$(export_blocks 12)
if echo "${safe_export}" | grep -q "Pruned"; then
  echo "[local-pruning-stage] unexpected Pruned during gentle policy" >&2
  exit 1
fi

log "stage 3: aggressive prune and Pruned response"
set_prune_policy 1 1 1 200
"${ICP_CANISTER_CALL[@]}" "${CANISTER_NAME}" prune_blocks '(1, 200)' >/dev/null
status=$(get_prune_status)
pruned_before="$(
  STATUS_JSON="${status}" python - <<'PY'
import json
import os
text = os.environ.get("STATUS_JSON", "")
data = json.loads(text)
value = data.get("pruned_before_block")
print("" if value is None else str(value))
PY
)"
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
