#!/usr/bin/env bash
# where: local dev CI entrypoint; what: run tests then deploy canister; why: keep steps deterministic and repeatable
set -euo pipefail

DFX_LOCAL_DIR="$HOME/Library/Application Support/org.dfinity.dfx/network/local"

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

cargo test -p evm-db -p ic-evm-core -p evm-canister

dfx deploy evm_canister

echo "[smoke] set_auto_mine(false)"
dfx canister call evm_canister set_auto_mine '(false)' >/dev/null

echo "[smoke] get_block(0)"
dfx canister call evm_canister get_block '(0)' >/dev/null

echo "[smoke] execute_ic_tx"
echo "[smoke] cycles before execute_ic_tx"
BEFORE_CYCLES=$(dfx canister call evm_canister get_cycle_balance --output json)
BEFORE_CYCLES_VAL=$(BEFORE_CYCLES="$BEFORE_CYCLES" python - <<'PY'
import os, re
text = os.environ.get("BEFORE_CYCLES", "")
m = re.search(r"(\\d+)", text)
print(m.group(1) if m else "0")
PY
)
TX_HEX=$(python - <<'PY'
version = b'\x01'
to = bytes.fromhex('0000000000000000000000000000000000000001')
value = (0).to_bytes(32, 'big')
gas = (500000).to_bytes(8, 'big')
nonce = (0).to_bytes(8, 'big')
data = b''
data_len = len(data).to_bytes(4, 'big')
tx = version + to + value + gas + nonce + data_len + data
print(tx.hex())
PY
)

EXEC_OUT=$(dfx canister call evm_canister execute_ic_tx "(vec { $(python - <<PY
tx = bytes.fromhex("$TX_HEX")
print('; '.join(str(b) for b in tx))
PY
) })")
echo "[smoke] cycles after execute_ic_tx"
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
echo "[smoke] execute_ic_tx cycles_used=${EXEC_COST}"

TX_ID=$(EXEC_OUT="$EXEC_OUT" python - <<'PY'
import os, re, sys
text = os.environ.get("EXEC_OUT", "")
if not re.search(r'variant\s*\{\s*(?:ok|Ok)\s*=', text):
    sys.stderr.write("[smoke] execute_ic_tx returned Err\\n")
    sys.stderr.write(text + "\\n")
    sys.exit(1)
m = re.search(r'tx_id\s*=\s*blob\s*\"([^\"]*)\"', text)
if not m:
    sys.stderr.write("[smoke] execute_ic_tx ok but tx_id not found\\n")
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

echo "[smoke] get_receipt(tx_id)"
dfx canister call evm_canister get_receipt "(vec { $TX_ID })" >/dev/null

echo "[smoke] get_block(1)"
dfx canister call evm_canister get_block '(1)' >/dev/null

echo "[smoke] submit_ic_tx -> produce_block"
SUBMIT_TX_ID=$(dfx canister call evm_canister submit_ic_tx "(vec { $(python - <<PY
tx = bytes.fromhex("$TX_HEX")
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
