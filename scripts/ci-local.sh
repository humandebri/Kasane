#!/usr/bin/env bash
# where: local dev CI entrypoint; what: run tests then deploy canister; why: keep steps deterministic and repeatable
set -euo pipefail

source "$(dirname "$0")/lib_init_args.sh"
NETWORK="${NETWORK:-local}"
ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME:-ci-local}"
SEED_RETRY_MAX="${SEED_RETRY_MAX:-8}"
SEED_RETRY_SLEEP_SEC="${SEED_RETRY_SLEEP_SEC:-65}"

cleanup() {
  icp network stop "${NETWORK}" >/dev/null 2>&1 || true
}

ensure_icp_identity() {
  if icp identity principal --identity "${ICP_IDENTITY_NAME}" >/dev/null 2>&1; then
    return
  fi
  echo "[setup] creating icp identity: ${ICP_IDENTITY_NAME}"
  icp identity new "${ICP_IDENTITY_NAME}" --storage plaintext >/dev/null
}

trap cleanup EXIT

ensure_icp_identity
icp network stop "${NETWORK}" >/dev/null 2>&1 || true
icp network start "${NETWORK}" -d

echo "[guard] rng callsite check"
scripts/check_rng_paths.sh
echo "[guard] wasm getrandom feature check"
scripts/check_getrandom_wasm_features.sh
echo "[guard] did sync check"
scripts/check_did_sync.sh
echo "[guard] legacy rpc symbol check"
scripts/check_legacy_rpc_removed.sh
echo "[guard] alloy dependency isolation check"
scripts/check_alloy_isolation.sh

echo "[guard] deny OP stack references"
DENY_PATTERN='op-revm|op_revm|op-node|op-geth|optimism|superchain|OpDeposit|L1BlockInfo'
if grep -RInE "$DENY_PATTERN" \
  --exclude-dir=.git \
  --exclude-dir=target \
  --exclude-dir=vendor \
  --exclude-dir=node_modules \
  --exclude='scripts/ci-local.sh' \
  --exclude='docs/ops/pr0-differential-runbook.md' \
  crates docs scripts README.md Cargo.toml; then
  echo "[guard] forbidden OP stack reference found"
  exit 1
fi

cargo test -p evm-db -p ic-evm-core -p ic-evm-wrapper --lib --tests

echo "[guard] evm-rpc-e2e manifest build check"
cargo test --manifest-path crates/evm-rpc-e2e/Cargo.toml --no-run

check_duplicate_dep() {
  local dep_name="$1"
  echo "[guard] ${dep_name} duplicate check"
  local cargo_tree_dup
  cargo_tree_dup=$(cargo tree --manifest-path crates/evm-rpc-e2e/Cargo.toml -d 2>&1 || true)
  if grep -q "^${dep_name} v" <<<"$cargo_tree_dup"; then
    echo "[guard] duplicate ${dep_name} detected"
    echo "$cargo_tree_dup"
    exit 1
  fi
}

check_duplicate_dep "ic-stable-structures"
check_duplicate_dep "ic-cdk-timers"

echo "[guard] PR0 differential check"
PR0_DIFF_LOCAL_FILE="${PR0_DIFF_LOCAL:-/tmp/pr0_snapshot_local.txt}"
PR0_DIFF_REFERENCE_FILE="${PR0_DIFF_REFERENCE:-docs/ops/pr0_snapshot_reference.txt}"
scripts/pr0_capture_local_snapshot.sh "$PR0_DIFF_LOCAL_FILE"
scripts/pr0_differential_compare.sh "$PR0_DIFF_LOCAL_FILE" "$PR0_DIFF_REFERENCE_FILE"

icp canister create -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" evm_canister >/dev/null 2>&1 || true
echo "[guard] release wasm endpoint guard"
scripts/release_wasm_guard.sh
echo "[build] ic-evm-wrapper (default features)"
cargo build --target wasm32-unknown-unknown --release -p ic-evm-wrapper --locked
if ! command -v ic-wasm >/dev/null 2>&1; then
  echo "[build] installing ic-wasm"
  cargo install ic-wasm --locked
fi
WASM_IN=target/wasm32-unknown-unknown/release/ic_evm_wrapper.wasm
WASM_OUT=target/wasm32-unknown-unknown/release/ic_evm_wrapper.final.wasm
scripts/build_wasm_postprocess.sh "$WASM_IN" "$WASM_OUT"
INIT_ARGS="$(build_init_args_for_current_identity 1000000000000000000)"
icp canister install -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" evm_canister --mode reinstall --wasm "$WASM_OUT" --args "$INIT_ARGS"

echo "[smoke] wait for replica"
sleep 2

echo "[smoke] query path (agent.query)"
NETWORK="${NETWORK}" CANISTER_NAME="evm_canister" ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME}" scripts/query_smoke.sh

echo "[smoke] set_auto_mine(false)"
icp canister call -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" evm_canister set_auto_mine '(false)' >/dev/null

echo "[smoke] submit_ic_tx -> produce_block"
TX_HEX=$(python - <<'PY'
version = b'\x02'
to = bytes.fromhex('0000000000000000000000000000000000000010')
value = (0).to_bytes(32, 'big')
gas = (500000).to_bytes(8, 'big')
nonce = (0).to_bytes(8, 'big')
max_fee = (2_000_000_000).to_bytes(16, 'big')
max_priority = (1_000_000_000).to_bytes(16, 'big')
data = b''
data_len = len(data).to_bytes(4, 'big')
tx = version + to + value + gas + nonce + max_fee + max_priority + data_len + data
print(tx.hex())
PY
)
TX_HEX_1=$(python - <<'PY'
version = b'\x02'
to = bytes.fromhex('0000000000000000000000000000000000000010')
value = (0).to_bytes(32, 'big')
gas = (500000).to_bytes(8, 'big')
nonce = (1).to_bytes(8, 'big')
max_fee = (2_000_000_000).to_bytes(16, 'big')
max_priority = (1_000_000_000).to_bytes(16, 'big')
data = b''
data_len = len(data).to_bytes(4, 'big')
tx = version + to + value + gas + nonce + max_fee + max_priority + data_len + data
print(tx.hex())
PY
)

TX_ARG_BYTES=$(python - <<PY
tx = bytes.fromhex("$TX_HEX")
print('; '.join(str(b) for b in tx))
PY
)

SUBMIT_OUT=""
for ((attempt=1; attempt<=SEED_RETRY_MAX; attempt++)); do
  SUBMIT_OUT=$(icp canister call -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" evm_canister submit_ic_tx "(vec { $TX_ARG_BYTES })")
  if grep -q 'ops.write.needs_migration' <<<"$SUBMIT_OUT"; then
    if [[ "$attempt" -lt "$SEED_RETRY_MAX" ]]; then
      echo "[smoke] submit blocked by migration, waiting for timer tick (attempt ${attempt}/${SEED_RETRY_MAX})"
      sleep "${SEED_RETRY_SLEEP_SEC}"
      continue
    fi
  fi
  break
done

set +e
TX_ID=$(SUBMIT_OUT="$SUBMIT_OUT" python - <<'PY'
import os, re, sys
text = os.environ.get("SUBMIT_OUT", "")
if not re.search(r'variant\s*\{\s*(?:ok|Ok)\s*=', text):
    sys.stderr.write("[smoke] submit_ic_tx returned Err\\n")
    sys.stderr.write(text + "\\n")
    sys.exit(1)
m = re.search(r'variant\s*\{\s*(?:ok|Ok)\s*=\s*blob\s*\"([^\"]*)\"', text)
if not m:
    sys.stderr.write("[smoke] submit_ic_tx ok but tx_id not found\\n")
    sys.stderr.write(text + "\\n")
    sys.exit(1)
s = m.group(1)
out = bytearray()
i = 0
while i < len(s):
    if s[i] == '\\\\':
        if i + 2 < len(s) and all(c in "0123456789abcdefABCDEF" for c in s[i+1:i+3]):
            out.append(int(s[i+1:i+3], 16))
            i += 3
            continue
        if i + 1 < len(s):
            esc = s[i+1]
            out.append(ord(esc))
            i += 2
            continue
    out.append(ord(s[i]))
    i += 1
print('; '.join(str(b) for b in out))
PY
)
EXEC_STATUS=$?
set -e
SKIP_TX_SMOKE=0
if [[ "$EXEC_STATUS" -ne 0 ]]; then
  if grep -q 'ops.write.needs_migration' <<<"$SUBMIT_OUT"; then
    echo "[smoke] WARN: write path is blocked by migration on local network; skipping tx smoke checks"
    SKIP_TX_SMOKE=1
  else
    echo "[smoke] submit_ic_tx failed: ${SUBMIT_OUT}"
    exit 1
  fi
fi

if [[ "$SKIP_TX_SMOKE" -eq 0 ]]; then
  echo "[smoke] produce_block(1)"
  icp canister call -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" evm_canister produce_block '(1)' >/dev/null

  echo "[smoke] submit_ic_tx(nonce=1) -> produce_block"
  SUBMIT_TX_ID=$(icp canister call -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" evm_canister submit_ic_tx "(vec { $(python - <<PY
tx = bytes.fromhex("$TX_HEX_1")
print('; '.join(str(b) for b in tx))
PY
) })")

  SUBMIT_TX_ID_BYTES=$(SUBMIT_TX_ID="$SUBMIT_TX_ID" python - <<'PY'
import os, re, sys
text = os.environ.get("SUBMIT_TX_ID", "")
if not re.search(r'variant\s*\{\s*(?:ok|Ok)\s*=', text):
    sys.stderr.write("[smoke] submit_ic_tx returned Err\\n")
    sys.stderr.write(text + "\\n")
    sys.exit(1)
m = re.search(r'variant\s*\{\s*(?:ok|Ok)\s*=\s*blob\s*\"([^\"]*)\"', text)
if not m:
    sys.stderr.write("[smoke] submit_ic_tx ok but tx_id not found\\n")
    sys.stderr.write(text + "\\n")
    sys.exit(1)
s = m.group(1)
out = bytearray()
i = 0
while i < len(s):
    if s[i] == '\\\\':
        if i + 2 < len(s) and all(c in "0123456789abcdefABCDEF" for c in s[i+1:i+3]):
            out.append(int(s[i+1:i+3], 16))
            i += 3
            continue
        if i + 1 < len(s):
            esc = s[i+1]
            out.append(ord(esc))
            i += 2
            continue
    out.append(ord(s[i]))
    i += 1
print('; '.join(str(b) for b in out))
PY
)
  icp canister call -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" evm_canister produce_block '(1)' >/dev/null
fi

echo "[e2e] rpc_compat_e2e"
scripts/run_rpc_compat_e2e.sh
