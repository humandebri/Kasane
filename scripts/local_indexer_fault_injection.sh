#!/usr/bin/env bash
# where: local failure-injection harness
# what: kill/restart indexer, tmp cleanup, network failure backoff checks
# why: 夜間運用で死ぬパターンを事前に潰すため
set -euo pipefail

NETWORK="${NETWORK:-local}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
INDEXER_IDLE_POLL_MS="${INDEXER_IDLE_POLL_MS:-1000}"
INDEXER_MAX_BYTES="${INDEXER_MAX_BYTES:-1200000}"
INDEXER_BACKOFF_MAX_MS="${INDEXER_BACKOFF_MAX_MS:-5000}"
INDEXER_CHAIN_ID="${INDEXER_CHAIN_ID:-4801360}"
WORKDIR="${WORKDIR:-$(mktemp -d -t ic-indexer-faults-)}"
INDEXER_LOG="${INDEXER_LOG:-${WORKDIR}/indexer.log}"
INDEXER_DATABASE_URL="${INDEXER_DATABASE_URL:-postgres://postgres:postgres@127.0.0.1:5432/ic_op_faults}"
INDEXER_ARCHIVE_DIR="${INDEXER_ARCHIVE_DIR:-${WORKDIR}/archive}"

DFX_CANISTER="dfx canister --network ${NETWORK}"

log() {
  echo "[local-indexer-faults] $*"
}

replica_api_host() {
  local host
  host=$(dfx info webserver-port 2>/dev/null || true)
  if [[ -n "${host}" ]]; then
    echo "http://127.0.0.1:${host}"
    return 0
  fi
  echo "http://127.0.0.1:4943"
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

require_cmd dfx
require_cmd python
require_cmd npm

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
  if ! ${DFX_CANISTER} call "${CANISTER_NAME}" health --output json >/dev/null 2>&1; then
    echo "[local-indexer-faults] canister not ready. run scripts/local_indexer_smoke.sh first." >&2
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

seed_block() {
  local nonce="$1"
  local tx_hex
  tx_hex=$(build_ic_tx_hex "${nonce}")
  local tx_bytes
  tx_bytes=$(hex_to_vec_bytes "${tx_hex}")
  ${DFX_CANISTER} call "${CANISTER_NAME}" submit_ic_tx "(vec { ${tx_bytes} })" >/dev/null
  ${DFX_CANISTER} call "${CANISTER_NAME}" produce_block '(1)' >/dev/null
}

start_indexer() {
  local canister_id
  local ic_host
  if [[ ! -d tools/indexer/node_modules ]]; then
    log "npm install (tools/indexer)"
    (cd tools/indexer && npm install)
  fi
  canister_id=$(${DFX_CANISTER} id "${CANISTER_NAME}")
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
  python - <<PY
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
}

log "workdir=${WORKDIR}"
ensure_database_exists
ensure_canister_ready

log "start indexer (initial)"
start_indexer
cursor_before=""
seed_block 2
seed_block 3
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

log "simulate network failure (dfx stop)"
dfx stop >/dev/null 2>&1 || true
sleep 4

log "verify retry/backoff"
assert_retry_backoff

log "restart dfx and wait recovery"
dfx start --background
sleep 3
seed_block 4
cursor_recovered=$(wait_for_cursor)
log "cursor_after_recover=${cursor_recovered}"

log "failure injection finished"
