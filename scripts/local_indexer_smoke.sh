#!/usr/bin/env bash
# where: local integration smoke harness
# what: deploy local canister, generate blocks, run indexer, verify cursor/archive/metrics/idle
# why: "設計は正しいが実接続で死ぬ"事故を最短で潰すため
set -euo pipefail
source "$(dirname "$0")/lib_init_args.sh"
source "$(dirname "$0")/lib_candid_result.sh"

NETWORK="${NETWORK:-local}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
MODE="${MODE:-reinstall}"
NETWORK_START="${NETWORK_START:-${DFX_START:-1}}"
NETWORK_CLEAN="${NETWORK_CLEAN:-${DFX_CLEAN:-1}}"
KEEP_NETWORK="${KEEP_NETWORK:-${KEEP_DFX:-0}}"
WORKDIR="${WORKDIR:-$(mktemp -d -t ic-indexer-smoke-)}"
INDEXER_LOG="${INDEXER_LOG:-${WORKDIR}/indexer.log}"
INDEXER_DATABASE_URL="${INDEXER_DATABASE_URL:-postgres://postgres:postgres@127.0.0.1:5432/ic_op_smoke}"
INDEXER_ARCHIVE_DIR="${INDEXER_ARCHIVE_DIR:-${WORKDIR}/archive}"
INDEXER_IDLE_POLL_MS="${INDEXER_IDLE_POLL_MS:-1000}"
INDEXER_MAX_BYTES="${INDEXER_MAX_BYTES:-1200000}"
INDEXER_BACKOFF_MAX_MS="${INDEXER_BACKOFF_MAX_MS:-5000}"
INDEXER_CHAIN_ID="${INDEXER_CHAIN_ID:-4801360}"
ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME:-ci-local}"
SEED_RETRY_MAX="${SEED_RETRY_MAX:-8}"
SEED_RETRY_SLEEP_SEC="${SEED_RETRY_SLEEP_SEC:-65}"
SEED_TRANSIENT_RETRY_SLEEP_SEC="${SEED_TRANSIENT_RETRY_SLEEP_SEC:-3}"
SEED_REQUIRED_HEAD_MIN="${SEED_REQUIRED_HEAD_MIN:-2}"

ICP_CANISTER_CALL=(icp canister call -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}")

log() {
  echo "[local-indexer-smoke] $*"
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

cleanup() {
  if [[ -n "${INDEXER_PID:-}" ]]; then
    kill "${INDEXER_PID}" >/dev/null 2>&1 || true
    wait "${INDEXER_PID}" >/dev/null 2>&1 || true
  fi
  if [[ "${KEEP_NETWORK}" != "1" ]]; then
    icp network stop "${NETWORK}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

report_failure() {
  local exit_code="$1"
  echo "[local-indexer-smoke] failed (exit=${exit_code})" >&2
  if [[ -f "${INDEXER_LOG}" ]]; then
    echo "[local-indexer-smoke] indexer.log tail:" >&2
    tail -n 80 "${INDEXER_LOG}" >&2 || true
  fi
}

trap 'rc=$?; report_failure "${rc}"; exit "${rc}"' ERR

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "[local-indexer-smoke] missing command: $1" >&2
    exit 1
  }
}

require_cmd icp
require_cmd cargo
require_cmd python
require_cmd npm

ensure_icp_identity() {
  if icp identity principal --identity "${ICP_IDENTITY_NAME}" >/dev/null 2>&1; then
    return
  fi
  log "creating icp identity: ${ICP_IDENTITY_NAME}"
  icp identity new "${ICP_IDENTITY_NAME}" --storage plaintext >/dev/null
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

start_network() {
  if [[ "${NETWORK_START}" != "1" ]]; then
    return
  fi
  if [[ "${NETWORK_CLEAN}" == "1" ]]; then
    icp network stop "${NETWORK}" >/dev/null 2>&1 || true
    icp network start "${NETWORK}" -d
  else
    icp network start "${NETWORK}" -d
  fi
}

build_and_install() {
  log "build wasm"
  cargo build --release --target wasm32-unknown-unknown -p ic-evm-wrapper

  if ! command -v ic-wasm >/dev/null 2>&1; then
    log "installing ic-wasm"
    cargo install ic-wasm --locked
  fi

  local wasm_in="target/wasm32-unknown-unknown/release/ic_evm_wrapper.wasm"
  local wasm_out="target/wasm32-unknown-unknown/release/ic_evm_wrapper.candid.wasm"
  ic-wasm "${wasm_in}" -o "${wasm_out}" metadata candid:service -f crates/ic-evm-wrapper/evm_canister.did

  icp canister create -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" "${CANISTER_NAME}" >/dev/null 2>&1 || true
  log "install wasm (mode=${MODE})"
  local init_args
  init_args="$(build_init_args_for_current_identity 1000000000000000000)"
  icp canister install -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --mode "${MODE}" --wasm "${wasm_out}" --args "${init_args}" "${CANISTER_NAME}"
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

seed_blocks() {
  log "set_auto_mine(false)"
  "${ICP_CANISTER_CALL[@]}" "${CANISTER_NAME}" set_auto_mine '(false)' >/dev/null

  submit_ic_tx_with_retry 0
  produce_seed_block 0
  submit_ic_tx_with_retry 1
  produce_seed_block 1
}

submit_ic_tx_with_retry() {
  local nonce="$1"
  local tx_hex
  tx_hex=$(build_ic_tx_hex "${nonce}")
  local tx_bytes
  tx_bytes=$(hex_to_vec_bytes "${tx_hex}")

  local attempt=1
  while [[ "${attempt}" -le "${SEED_RETRY_MAX}" ]]; do
    log "submit_ic_tx(nonce=${nonce}) attempt=${attempt}/${SEED_RETRY_MAX}"
    local out
    local rc
    set +e
    out=$("${ICP_CANISTER_CALL[@]}" "${CANISTER_NAME}" submit_ic_tx "(vec { ${tx_bytes} })" 2>&1)
    rc=$?
    set -e

    if [[ "${rc}" -eq 0 ]] && candid_is_ok "${out}"; then
      log "submit_ic_tx(nonce=${nonce}) accepted"
      return 0
    fi

    if grep -q "ops.write.needs_migration" <<<"${out}"; then
      if [[ "${attempt}" -ge "${SEED_RETRY_MAX}" ]]; then
        echo "[local-indexer-smoke] seed failed: migration not finished after ${SEED_RETRY_MAX} attempts" >&2
        echo "[local-indexer-smoke] last submit output: ${out}" >&2
        return 1
      fi
      log "submit_ic_tx(nonce=${nonce}) needs migration; sleep ${SEED_RETRY_SLEEP_SEC}s then retry"
      sleep "${SEED_RETRY_SLEEP_SEC}"
      attempt=$((attempt + 1))
      continue
    fi

    if grep -Eqi "error sending request|communication with the replica|connection refused|timed out|incompletemessage" <<<"${out}"; then
      if [[ "${attempt}" -ge "${SEED_RETRY_MAX}" ]]; then
        echo "[local-indexer-smoke] seed failed: transport error did not recover after ${SEED_RETRY_MAX} attempts" >&2
        echo "[local-indexer-smoke] last submit output: ${out}" >&2
        return 1
      fi
      log "submit_ic_tx(nonce=${nonce}) transient transport error; sleep ${SEED_TRANSIENT_RETRY_SLEEP_SEC}s then retry"
      sleep "${SEED_TRANSIENT_RETRY_SLEEP_SEC}"
      attempt=$((attempt + 1))
      continue
    fi

    echo "[local-indexer-smoke] seed failed: submit_ic_tx returned unexpected result (nonce=${nonce})" >&2
    echo "[local-indexer-smoke] submit output: ${out}" >&2
    return 1
  done
}

produce_seed_block() {
  local nonce="$1"
  local out
  out=$("${ICP_CANISTER_CALL[@]}" "${CANISTER_NAME}" produce_block '(1)' 2>&1)
  if ! candid_is_ok "${out}"; then
    echo "[local-indexer-smoke] seed failed: produce_block after nonce=${nonce} did not return Ok" >&2
    echo "[local-indexer-smoke] produce_block output: ${out}" >&2
    return 1
  fi
}

run_query_smoke_strict() {
  log "run strict query smoke (required_head_min=${SEED_REQUIRED_HEAD_MIN})"
  NETWORK="${NETWORK}" \
  CANISTER_NAME="${CANISTER_NAME}" \
  ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME}" \
  QUERY_SMOKE_ALLOW_EXPORT_MISSING_DATA="false" \
  QUERY_SMOKE_REQUIRED_HEAD_MIN="${SEED_REQUIRED_HEAD_MIN}" \
  scripts/query_smoke.sh
}

start_indexer() {
  if [[ ! -d tools/indexer/node_modules ]]; then
    log "npm install (tools/indexer)"
    (cd tools/indexer && npm install)
  fi

  local canister_id
  local ic_host
  canister_id=$(icp canister status -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --id-only "${CANISTER_NAME}")
  ic_host=$(replica_api_host)
  log "start indexer (canister_id=${canister_id})"
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

wait_for_cursor() {
  local deadline
  deadline=$((SECONDS + 60))
  while [[ ${SECONDS} -lt ${deadline} ]]; do
    local value
    value=$(INDEXER_DATABASE_URL="${INDEXER_DATABASE_URL}" node - <<'NODE'
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
)
    if [[ -n "${value}" ]]; then
      log "cursor detected block_number=${value}"
      return 0
    fi
    sleep 1
  done
  return 1
}

assert_archive_grows() {
  local count
  count=$(find "${INDEXER_ARCHIVE_DIR}" -type f -name "*.bundle.zst" 2>/dev/null | wc -l | tr -d ' ')
  if [[ "${count}" -lt 1 ]]; then
    echo "[local-indexer-smoke] archive missing: ${INDEXER_ARCHIVE_DIR}" >&2
    return 1
  fi
  log "archive files=${count}"
}

assert_metrics_daily() {
  INDEXER_DATABASE_URL="${INDEXER_DATABASE_URL}" node - <<'NODE'
const { Client } = require('./tools/indexer/node_modules/pg');
(async () => {
  const client = new Client({ connectionString: process.env.INDEXER_DATABASE_URL });
  await client.connect();
  try {
    const row = await client.query("select blocks_ingested, raw_bytes, compressed_bytes, archive_bytes from metrics_daily limit 1");
    if (row.rowCount === 0) process.exit(1);
    const data = row.rows[0];
    const blocks = Number(data.blocks_ingested ?? 0);
    if (!Number.isFinite(blocks) || blocks < 1) process.exit(2);
    if (data.archive_bytes === null || data.archive_bytes === undefined) process.exit(3);
    console.log(`blocks_ingested=${data.blocks_ingested} raw_bytes=${data.raw_bytes} compressed_bytes=${data.compressed_bytes} archive_bytes=${data.archive_bytes}`);
  } finally {
    await client.end();
  }
})().catch(() => process.exit(4));
NODE
}

wait_for_idle_log() {
  local deadline
  deadline=$((SECONDS + 90))
  while [[ ${SECONDS} -lt ${deadline} ]]; do
    if grep -q '"event":"idle"' "${INDEXER_LOG}" 2>/dev/null; then
      log "idle log detected"
      return 0
    fi
    sleep 2
  done
  return 1
}

log "workdir=${WORKDIR}"
ensure_icp_identity
ensure_database_exists
start_network
build_and_install
NETWORK="${NETWORK}" CANISTER_NAME="${CANISTER_NAME}" ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME}" scripts/query_smoke.sh
"${ICP_CANISTER_CALL[@]}" "${CANISTER_NAME}" set_pruning_enabled '(false)' >/dev/null
seed_blocks
run_query_smoke_strict
start_indexer

log "wait for cursor advance"
wait_for_cursor
log "check archive"
assert_archive_grows
log "check metrics_daily"
assert_metrics_daily
log "wait for idle"
wait_for_idle_log

if grep -q '"event":"fatal"' "${INDEXER_LOG}" 2>/dev/null; then
  echo "[local-indexer-smoke] fatal log detected" >&2
  exit 1
fi

log "local integration smoke finished"
