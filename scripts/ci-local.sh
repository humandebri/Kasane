#!/usr/bin/env bash
# where: local dev CI entrypoint; what: run tests then deploy canister; why: keep steps deterministic and repeatable
set -euo pipefail

DFX_LOCAL_DIR="$HOME/Library/Application Support/org.dfinity.dfx/network/local"
source "$(dirname "$0")/lib_init_args.sh"

cleanup() {
  dfx stop >/dev/null 2>&1 || true
}

trap cleanup EXIT

if [[ -f "$DFX_LOCAL_DIR/pid" ]]; then
  kill -9 "$(cat "$DFX_LOCAL_DIR/pid")" >/dev/null 2>&1 || true
  rm -f "$DFX_LOCAL_DIR/pid"
fi
if [[ -f "$DFX_LOCAL_DIR/pocket-ic-pid" ]]; then
  kill -9 "$(cat "$DFX_LOCAL_DIR/pocket-ic-pid")" >/dev/null 2>&1 || true
  rm -f "$DFX_LOCAL_DIR/pocket-ic-pid"
fi

dfx start --clean --background

echo "[guard] rng callsite check"
scripts/check_rng_paths.sh
echo "[guard] wasm getrandom feature check"
scripts/check_getrandom_wasm_features.sh
echo "[guard] did sync check"
scripts/check_did_sync.sh

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

cargo test -p evm-db -p ic-evm-core -p ic-evm-wrapper

echo "[guard] PR0 differential check"
PR0_DIFF_LOCAL_FILE="${PR0_DIFF_LOCAL:-/tmp/pr0_snapshot_local.txt}"
PR0_DIFF_REFERENCE_FILE="${PR0_DIFF_REFERENCE:-docs/ops/pr0_snapshot_reference.txt}"
scripts/pr0_capture_local_snapshot.sh "$PR0_DIFF_LOCAL_FILE"
scripts/pr0_differential_compare.sh "$PR0_DIFF_LOCAL_FILE" "$PR0_DIFF_REFERENCE_FILE"

dfx canister create evm_canister
echo "[build] ic-evm-wrapper with dev-faucet"
cargo build --target wasm32-unknown-unknown --release -p ic-evm-wrapper --features dev-faucet --locked
if ! command -v ic-wasm >/dev/null 2>&1; then
  echo "[build] installing ic-wasm"
  cargo install ic-wasm --locked
fi
WASM_IN=target/wasm32-unknown-unknown/release/ic_evm_wrapper.wasm
WASM_OUT=target/wasm32-unknown-unknown/release/ic_evm_wrapper.candid.wasm
ic-wasm "$WASM_IN" -o "$WASM_OUT" metadata candid:service -f crates/ic-evm-wrapper/evm_canister.did
INIT_ARGS="$(build_init_args_for_current_identity 1000000000000000000)"
dfx canister install evm_canister --wasm "$WASM_OUT" --argument "$INIT_ARGS"

echo "[smoke] wait for replica"
for i in {1..10}; do
  if dfx canister call evm_canister health --output json >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

echo "[smoke] dev_mint caller"
CALLER_PRINCIPAL=$(dfx identity get-principal)
CALLER_HEX=$(cargo run -q -p ic-evm-core --bin caller_evm -- "$CALLER_PRINCIPAL")
CALLER_BLOB=$(python - <<PY
data = bytes.fromhex("$CALLER_HEX")
print(''.join(f'\\\\{b:02x}' for b in data))
PY
)
dfx canister call evm_canister dev_mint "(blob \"$CALLER_BLOB\", 1000000000000000000:nat)" >/dev/null

echo "[smoke] set_auto_mine(false)"
dfx canister call evm_canister set_auto_mine '(false)' >/dev/null

echo "[smoke] get_block(0)"
dfx canister call evm_canister get_block '(0)' >/dev/null

echo "[smoke] submit_ic_tx -> produce_block"
echo "[smoke] cycles before submit_ic_tx"
BEFORE_CYCLES=$(dfx canister call evm_canister get_cycle_balance --output json)
BEFORE_CYCLES_VAL=$(BEFORE_CYCLES="$BEFORE_CYCLES" python - <<'PY'
import os, re
text = os.environ.get("BEFORE_CYCLES", "")
m = re.search(r"(\\d+)", text)
print(m.group(1) if m else "0")
PY
)
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

SUBMIT_OUT=$(dfx canister call evm_canister submit_ic_tx "(vec { $(python - <<PY
tx = bytes.fromhex("$TX_HEX")
print('; '.join(str(b) for b in tx))
PY
) })")

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
if [[ "$EXEC_STATUS" -ne 0 ]]; then
  echo "[smoke] submit_ic_tx failed, dumping health/metrics"
  dfx canister call evm_canister health --output json || true
  dfx canister call evm_canister metrics '(60)' --output json || true
  dfx canister call evm_canister get_block '(0)' --output json || true
  dfx canister call evm_canister get_block '(1)' --output json || true
  dfx canister call evm_canister get_queue_snapshot '(5, null)' --output json || true
  exit 1
fi

echo "[smoke] produce_block(1)"
dfx canister call evm_canister produce_block '(1)' >/dev/null

echo "[smoke] cycles after produce_block"
AFTER_CYCLES=$(dfx canister call evm_canister get_cycle_balance --output json)
AFTER_CYCLES_VAL=$(AFTER_CYCLES="$AFTER_CYCLES" python - <<'PY'
import os, re
text = os.environ.get("AFTER_CYCLES", "")
m = re.search(r"(\\d+)", text)
print(m.group(1) if m else "0")
PY
)
EXEC_COST=$(python - <<PY
before = int("$BEFORE_CYCLES_VAL")
after = int("$AFTER_CYCLES_VAL")
print(before - after if before >= after else 0)
PY
)
echo "[smoke] submit+produce cycles_used=${EXEC_COST}"

echo "[smoke] get_receipt(tx_id)"
dfx canister call evm_canister get_receipt "(vec { $TX_ID })" >/dev/null

echo "[smoke] get_block(1)"
dfx canister call evm_canister get_block '(1)' >/dev/null

echo "[smoke] submit_ic_tx(nonce=1) -> produce_block"
SUBMIT_TX_ID=$(dfx canister call evm_canister submit_ic_tx "(vec { $(python - <<PY
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

echo "[smoke] get_pending(tx_id)"
dfx canister call evm_canister get_pending "(vec { $SUBMIT_TX_ID_BYTES })" >/dev/null

echo "[smoke] get_queue_snapshot(1)"
QUEUE_OUT=$(dfx canister call evm_canister get_queue_snapshot '(1, null)' --output json)
HAS_ITEM=$(QUEUE_OUT="$QUEUE_OUT" python - <<'PY'
import json, os
data = json.loads(os.environ.get("QUEUE_OUT", "null"))
items = data.get("items", [])
print("1" if isinstance(items, list) and len(items) > 0 else "0")
PY
)

if [[ "$HAS_ITEM" == "1" ]]; then
  dfx canister call evm_canister produce_block '(1)' >/dev/null
  dfx canister call evm_canister get_receipt "(vec { $SUBMIT_TX_ID_BYTES })" >/dev/null
else
  echo "[smoke] queue empty, skipping produce_block"
fi

echo "[e2e] rpc_compat_e2e"
scripts/run_rpc_compat_e2e.sh
