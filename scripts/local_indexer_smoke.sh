#!/usr/bin/env bash
# where: local integration smoke harness
# what: deploy local canister, generate blocks, run indexer, verify cursor/archive/metrics/idle
# why: "設計は正しいが実接続で死ぬ"事故を最短で潰すため
set -euo pipefail
source "$(dirname "$0")/lib_init_args.sh"

NETWORK="${NETWORK:-local}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
MODE="${MODE:-reinstall}"
DFX_START="${DFX_START:-1}"
DFX_CLEAN="${DFX_CLEAN:-1}"
KEEP_DFX="${KEEP_DFX:-0}"
WORKDIR="${WORKDIR:-$(mktemp -d -t ic-indexer-smoke-)}"
INDEXER_LOG="${INDEXER_LOG:-${WORKDIR}/indexer.log}"
INDEXER_DB_PATH="${INDEXER_DB_PATH:-${WORKDIR}/indexer.sqlite}"
INDEXER_ARCHIVE_DIR="${INDEXER_ARCHIVE_DIR:-${WORKDIR}/archive}"
INDEXER_IDLE_POLL_MS="${INDEXER_IDLE_POLL_MS:-1000}"
INDEXER_MAX_BYTES="${INDEXER_MAX_BYTES:-1200000}"
INDEXER_BACKOFF_MAX_MS="${INDEXER_BACKOFF_MAX_MS:-5000}"
INDEXER_CHAIN_ID="${INDEXER_CHAIN_ID:-4801360}"
ENABLE_DEV_FAUCET="${ENABLE_DEV_FAUCET:-1}"

DFX_CANISTER="dfx canister --network ${NETWORK}"

log() {
  echo "[local-indexer-smoke] $*"
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
  if [[ "${KEEP_DFX}" != "1" ]]; then
    dfx stop >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "[local-indexer-smoke] missing command: $1" >&2
    exit 1
  }
}

require_cmd dfx
require_cmd cargo
require_cmd python
require_cmd npm

start_dfx() {
  if [[ "${DFX_START}" != "1" ]]; then
    return
  fi
  if [[ "${DFX_CLEAN}" == "1" ]]; then
    dfx stop >/dev/null 2>&1 || true
    local dfx_local_dir="$HOME/Library/Application Support/org.dfinity.dfx/network/local"
    if [[ -f "${dfx_local_dir}/pid" ]]; then
      kill -9 "$(cat "${dfx_local_dir}/pid")" >/dev/null 2>&1 || true
      rm -f "${dfx_local_dir}/pid"
    fi
    if [[ -f "${dfx_local_dir}/pocket-ic-pid" ]]; then
      kill -9 "$(cat "${dfx_local_dir}/pocket-ic-pid")" >/dev/null 2>&1 || true
      rm -f "${dfx_local_dir}/pocket-ic-pid"
    fi
    dfx start --clean --background
  else
    dfx start --background
  fi
}

build_and_install() {
  log "build wasm (dev-faucet=${ENABLE_DEV_FAUCET})"
  local -a cargo_args
  cargo_args=(--release --target wasm32-unknown-unknown -p ic-evm-wrapper)
  if [[ "${ENABLE_DEV_FAUCET}" == "1" ]]; then
    cargo_args+=(--features dev-faucet)
  fi
  cargo build "${cargo_args[@]}"

  if ! command -v ic-wasm >/dev/null 2>&1; then
    log "installing ic-wasm"
    cargo install ic-wasm --locked
  fi

  local wasm_in="target/wasm32-unknown-unknown/release/ic_evm_wrapper.wasm"
  local wasm_out="target/wasm32-unknown-unknown/release/ic_evm_wrapper.candid.wasm"
  ic-wasm "${wasm_in}" -o "${wasm_out}" metadata candid:service -f crates/ic-evm-wrapper/evm_canister.did

  ${DFX_CANISTER} create "${CANISTER_NAME}" >/dev/null 2>&1 || true
  log "install wasm (mode=${MODE})"
  local init_args
  init_args="$(build_init_args_for_current_identity 1000000000000000000)"
  printf "yes\n" | ${DFX_CANISTER} install --mode "${MODE}" --wasm "${wasm_out}" --argument "${init_args}" "${CANISTER_NAME}"
}

caller_blob() {
  local principal
  principal=$(dfx identity get-principal)
  local hex
  hex=$(cargo run -q -p ic-evm-core --bin caller_evm -- "${principal}")
  python - <<PY
import sys
hex_str = "${hex}".strip()
data = bytes.fromhex(hex_str)
print(''.join(f'\\\\{b:02x}' for b in data))
PY
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
  log "dev_mint for caller"
  local caller
  caller=$(caller_blob)
  ${DFX_CANISTER} call "${CANISTER_NAME}" dev_mint "(blob \"${caller}\", 1000000000000000000:nat)" >/dev/null

  log "set_auto_mine(false)"
  ${DFX_CANISTER} call "${CANISTER_NAME}" set_auto_mine '(false)' >/dev/null

  log "submit_ic_tx -> produce_block"
  local tx_hex
  tx_hex=$(build_ic_tx_hex 0)
  local tx_bytes
  tx_bytes=$(hex_to_vec_bytes "${tx_hex}")
  ${DFX_CANISTER} call "${CANISTER_NAME}" submit_ic_tx "(vec { ${tx_bytes} })" >/dev/null
  ${DFX_CANISTER} call "${CANISTER_NAME}" produce_block '(1)' >/dev/null

  tx_hex=$(build_ic_tx_hex 1)
  tx_bytes=$(hex_to_vec_bytes "${tx_hex}")
  ${DFX_CANISTER} call "${CANISTER_NAME}" submit_ic_tx "(vec { ${tx_bytes} })" >/dev/null
  ${DFX_CANISTER} call "${CANISTER_NAME}" produce_block '(1)' >/dev/null
}

start_indexer() {
  if [[ ! -d tools/indexer/node_modules ]]; then
    log "npm install (tools/indexer)"
    (cd tools/indexer && npm install)
  fi

  local canister_id
  local ic_host
  canister_id=$(${DFX_CANISTER} id "${CANISTER_NAME}")
  ic_host=$(replica_api_host)
  log "start indexer (canister_id=${canister_id})"
  mkdir -p "${INDEXER_ARCHIVE_DIR}"
  (
    cd tools/indexer
    INDEXER_CANISTER_ID="${canister_id}" \
    INDEXER_IC_HOST="${ic_host}" \
    INDEXER_DB_PATH="${INDEXER_DB_PATH}" \
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
    if [[ -f "${INDEXER_DB_PATH}" ]]; then
      local value
      value=$(python - <<PY
import json, sqlite3, sys
path = "${INDEXER_DB_PATH}"
conn = sqlite3.connect(path)
row = conn.execute("select value from meta where key = 'cursor'").fetchone()
conn.close()
if not row:
  print("")
  sys.exit(0)
text = row[0]
try:
  data = json.loads(text)
  print(data.get("block_number", ""))
except Exception:
  print("")
PY
)
      if [[ -n "${value}" ]]; then
        log "cursor detected block_number=${value}"
        return 0
      fi
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
  python - <<PY
import sqlite3, sys
path = "${INDEXER_DB_PATH}"
conn = sqlite3.connect(path)
row = conn.execute("select blocks_ingested, raw_bytes, compressed_bytes, sqlite_bytes, archive_bytes from metrics_daily limit 1").fetchone()
conn.close()
if not row:
  sys.exit(1)
blocks, raw_b, comp_b, sqlite_b, archive_b = row
if blocks is None or blocks < 1:
  sys.exit(2)
if sqlite_b is None or archive_b is None:
  sys.exit(3)
print(f"blocks_ingested={blocks} raw_bytes={raw_b} compressed_bytes={comp_b} sqlite_bytes={sqlite_b} archive_bytes={archive_b}")
PY
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
start_dfx
build_and_install
${DFX_CANISTER} call "${CANISTER_NAME}" set_pruning_enabled '(false)' >/dev/null
seed_blocks
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
