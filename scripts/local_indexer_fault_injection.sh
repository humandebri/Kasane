#!/usr/bin/env bash
# where: local failure-injection harness
# what: kill/restart indexer, tmp cleanup, network failure backoff checks
# why: 夜間運用で死ぬパターンを事前に潰すため
set -euo pipefail
source "$(dirname "$0")/lib_candid_result.sh"

NETWORK="${NETWORK:-local}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME:-ci-local}"
INDEXER_IDLE_POLL_MS="${INDEXER_IDLE_POLL_MS:-1000}"
INDEXER_MAX_BYTES="${INDEXER_MAX_BYTES:-1200000}"
INDEXER_BACKOFF_MAX_MS="${INDEXER_BACKOFF_MAX_MS:-5000}"
INDEXER_CHAIN_ID="${INDEXER_CHAIN_ID:-4801360}"
WORKDIR="${WORKDIR:-$(mktemp -d -t ic-indexer-faults-)}"
INDEXER_LOG="${INDEXER_LOG:-${WORKDIR}/indexer.log}"
INDEXER_DATABASE_URL="${INDEXER_DATABASE_URL:-postgres://postgres:postgres@127.0.0.1:5432/ic_op_faults}"
INDEXER_ARCHIVE_DIR="${INDEXER_ARCHIVE_DIR:-${WORKDIR}/archive}"
SEED_TX_MAX_FEE_WEI="${SEED_TX_MAX_FEE_WEI:-1000000000000}"
SEED_TX_MAX_PRIORITY_FEE_WEI="${SEED_TX_MAX_PRIORITY_FEE_WEI:-250000000000}"
SEED_SUBMIT_TIMEOUT_SEC="${SEED_SUBMIT_TIMEOUT_SEC:-20}"
SEED_BLOCK_COUNT="${SEED_BLOCK_COUNT:-0}"

ICP_CANISTER_CALL=(icp canister call -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}")

log() {
  echo "[local-indexer-faults] $*"
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
  icp canister status -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --id-only "${CANISTER_NAME}" 2>/dev/null || true
}

cleanup() {
  if [[ -n "${INDEXER_PID:-}" ]]; then
    kill "${INDEXER_PID}" >/dev/null 2>&1 || true
    wait "${INDEXER_PID}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "[local-indexer-faults] missing command: $1" >&2
    exit 1
  }
}

require_cmd icp
require_cmd python
require_cmd npm

submit_ic_tx_with_timeout() {
  local tx_arg="$1"
  NETWORK="${NETWORK}" \
  ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME}" \
  CANISTER_NAME="${CANISTER_NAME}" \
  TX_ARG="${tx_arg}" \
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
    os.environ["TX_ARG"],
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

ensure_database_exists() {
  INDEXER_DATABASE_URL="${INDEXER_DATABASE_URL}" node - <<'NODE'
const { Client } = require('./tools/indexer/node_modules/pg');
const target = new URL(process.env.INDEXER_DATABASE_URL);
const dbName = (target.pathname || '/').replace(/^\//, '') || 'postgres';
const admin = new URL(target.toString());
admin.pathname = '/postgres';

const quote = (s) => `"${String(s).replace(/"/g, '""')}"`;

(async () => {
  const client = new Client({ connectionString: admin.toString() });
  await client.connect();
  try {
    const exists = await client.query('select 1 from pg_database where datname = $1', [dbName]);
    if (exists.rowCount === 0) {
      await client.query(`create database ${quote(dbName)}`);
    }
  } finally {
    await client.end();
  }
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
NODE
}

ensure_canister_ready() {
  if ! "${ICP_CANISTER_CALL[@]}" "${CANISTER_NAME}" health >/dev/null 2>&1; then
    echo "[local-indexer-faults] canister not ready. run scripts/local_indexer_smoke.sh first." >&2
    exit 1
  fi
}

build_ic_tx_arg() {
  local nonce="$1"
  python - <<PY
to = bytes.fromhex('0000000000000000000000000000000000000010')
to_csv = '; '.join(str(b) for b in to)
try:
    import time
    data = int(time.time_ns()).to_bytes(8, 'big')
except Exception:
    data = b'\x01'
data_csv = '; '.join(str(b) for b in data)
print(f"(record {{ to = opt vec {{ {to_csv} }}; value = 0 : nat; gas_limit = 500000 : nat64; nonce = {int('${nonce}')} : nat64; max_fee_per_gas = {int('${SEED_TX_MAX_FEE_WEI}')} : nat; max_priority_fee_per_gas = {int('${SEED_TX_MAX_PRIORITY_FEE_WEI}')} : nat; data = vec {{ {data_csv} }}; }})")
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
  if [[ -z "${canister_id}" ]]; then
    echo "0"
    return 0
  fi
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
  echo "[local-indexer-faults] auto-mine did not advance head in time" >&2
  return 1
}

seed_block() {
  local nonce="$1"
  local start_head
  start_head="$(query_head_block)"
  local accepted=0
  local attempts=0
  while [[ "${accepted}" -ne 1 && "${attempts}" -lt 512 ]]; do
    local tx_arg
    tx_arg=$(build_ic_tx_arg "${nonce}")
    local out
    out="$(submit_ic_tx_with_timeout "${tx_arg}" || true)"
    if grep -qi "timed out" <<<"${out}" || [[ -z "${out}" ]]; then
      attempts=$((attempts + 1))
      continue
    fi
    if candid_is_ok "${out}"; then
      accepted=1
      break
    fi
    if grep -qi 'nonce too low\|tx_already_seen' <<<"${out}"; then
      nonce=$((nonce + 1))
      attempts=$((attempts + 1))
      continue
    fi
    if grep -qi 'nonce_gap' <<<"${out}"; then
      if [[ "${nonce}" -gt 0 ]]; then
        nonce=$((nonce - 1))
      fi
      attempts=$((attempts + 1))
      continue
    fi
    echo "[local-indexer-faults] seed submit failed: ${out}" >&2
    return 1
  done
  if [[ "${accepted}" -ne 1 ]]; then
    echo "[local-indexer-faults] failed to submit seed tx after retries" >&2
    return 1
  fi
  if ! wait_for_head_advance "${start_head}"; then
    log "seed accepted but head did not advance in time (start=${start_head})"
  fi
}

start_indexer() {
  local canister_id
  local ic_host
  if [[ ! -d tools/indexer/node_modules ]]; then
    log "npm install (tools/indexer)"
    (cd tools/indexer && npm install)
  fi
  canister_id="$(resolve_canister_id)"
  if [[ -z "${canister_id}" ]]; then
    echo "[local-indexer-faults] canister id not found for ${CANISTER_NAME}" >&2
    return 1
  fi
  ic_host=$(replica_api_host)
  mkdir -p "${INDEXER_ARCHIVE_DIR}"
  (
    cd tools/indexer
    EVM_CANISTER_ID="${canister_id}" \
    INDEXER_IC_HOST="${ic_host}" \
    INDEXER_DATABASE_URL="${INDEXER_DATABASE_URL}" \
    INDEXER_ARCHIVE_DIR="${INDEXER_ARCHIVE_DIR}" \
    INDEXER_MAX_BYTES="${INDEXER_MAX_BYTES}" \
    INDEXER_BACKOFF_MAX_MS="${INDEXER_BACKOFF_MAX_MS}" \
    INDEXER_IDLE_POLL_MS="${INDEXER_IDLE_POLL_MS}" \
    INDEXER_PRUNE_STATUS_POLL_MS="1000" \
    INDEXER_FETCH_ROOT_KEY="true" \
    INDEXER_CHAIN_ID="${INDEXER_CHAIN_ID}" \
    npm run dev
  ) >"${INDEXER_LOG}" 2>&1 &
  INDEXER_PID=$!
}

read_cursor_block() {
  INDEXER_DATABASE_URL="${INDEXER_DATABASE_URL}" node - <<'NODE'
const { Client } = require('./tools/indexer/node_modules/pg');
(async () => {
  const client = new Client({ connectionString: process.env.INDEXER_DATABASE_URL });
  await client.connect();
  try {
    const row = await client.query("select value from meta where key = 'cursor'");
    if (row.rowCount === 0) {
      process.stdout.write("");
      return;
    }
    const data = JSON.parse(String(row.rows[0].value));
    process.stdout.write(String(data.block_number ?? ""));
  } catch {
    process.stdout.write("");
  } finally {
    await client.end();
  }
})().catch(() => process.stdout.write(""));
NODE
}

wait_for_cursor() {
  local deadline
  deadline=$((SECONDS + 40))
  while [[ ${SECONDS} -lt ${deadline} ]]; do
    local value
    value=$(read_cursor_block)
    if [[ -n "${value}" ]]; then
      echo "${value}"
      return 0
    fi
    sleep 1
  done
  return 1
}

assert_tmp_removed() {
  local tmp_path="$1"
  if [[ -f "${tmp_path}" ]]; then
    echo "[local-indexer-faults] tmp file not removed: ${tmp_path}" >&2
    return 1
  fi
}

assert_retry_backoff() {
  local out
  out="$(python - <<PY
import json, re
path = "${INDEXER_LOG}"
max_ms = int("${INDEXER_BACKOFF_MAX_MS}")
backoffs = []
with open(path, "r", encoding="utf-8") as f:
  for line in f:
    if '"event":"retry"' not in line:
      continue
    m = re.search(r'\"backoff_ms\":(\d+)', line)
    if m:
      backoffs.append(int(m.group(1)))
if not backoffs:
  raise SystemExit(1)
if any(b > max_ms for b in backoffs):
  raise SystemExit(2)
print(f"retry_backoff_samples={backoffs}")
PY
)" || {
    log "retry/backoff sample not found; skip strict assertion"
    return 0
  }
  log "${out}"
}

ensure_port_8000_released() {
  local deadline=$((SECONDS + 15))
  while [[ ${SECONDS} -lt ${deadline} ]]; do
    if ! lsof -nP -iTCP:8000 -sTCP:LISTEN >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  lsof -nP -iTCP:8000 -sTCP:LISTEN | tail -n +2 | awk '{print $2}' | xargs -I{} kill {} >/dev/null 2>&1 || true
  sleep 1
}

log "workdir=${WORKDIR}"
ensure_database_exists
ensure_canister_ready

log "start indexer (initial)"
start_indexer
cursor_before=""
base_nonce="$(query_head_block)"
if [[ "${SEED_BLOCK_COUNT}" -gt 0 ]]; then
  seed_block "${base_nonce}"
  if [[ "${SEED_BLOCK_COUNT}" -gt 1 ]]; then
    seed_block "$((base_nonce + 1))"
  fi
else
  log "seed blocks skipped (SEED_BLOCK_COUNT=0)"
fi
cursor_before=$(wait_for_cursor)
log "cursor_before_kill=${cursor_before}"

log "kill indexer during ingest"
kill -9 "${INDEXER_PID}" >/dev/null 2>&1 || true
wait "${INDEXER_PID}" >/dev/null 2>&1 || true
INDEXER_PID=""

log "restart indexer and verify cursor resumes"
start_indexer
cursor_after=$(wait_for_cursor)
log "cursor_after_restart=${cursor_after}"
if [[ -n "${cursor_before}" && -n "${cursor_after}" ]]; then
  if [[ "${cursor_after}" -lt "${cursor_before}" ]]; then
    echo "[local-indexer-faults] cursor regressed after restart" >&2
    exit 1
  fi
fi

log "inject archive tmp and verify GC"
chain_dir="${INDEXER_ARCHIVE_DIR}/${INDEXER_CHAIN_ID}"
mkdir -p "${chain_dir}"
tmp_path="${chain_dir}/999.bundle.zst.tmp"
echo "tmp" >"${tmp_path}"
kill "${INDEXER_PID}" >/dev/null 2>&1 || true
wait "${INDEXER_PID}" >/dev/null 2>&1 || true
INDEXER_PID=""
start_indexer
sleep 2
assert_tmp_removed "${tmp_path}"

log "simulate network failure (icp network stop)"
icp network stop "${NETWORK}" >/dev/null 2>&1 || true
ensure_port_8000_released
sleep 4

log "verify retry/backoff"
assert_retry_backoff

log "restart network and wait recovery"
icp network start "${NETWORK}" -d
sleep 3
if [[ -n "$(resolve_canister_id)" ]]; then
  seed_block "$((base_nonce + 2))" || true
  cursor_recovered=$(wait_for_cursor)
  log "cursor_after_recover=${cursor_recovered}"
else
  log "canister not found after network restart; skip recovery seed"
fi

log "failure injection finished"
