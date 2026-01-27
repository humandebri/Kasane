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

cargo test -p evm-db -p evm-core -p evm-canister

dfx deploy evm_canister

echo "[smoke] get_block(0)"
dfx canister call evm_canister get_block '(0)' >/dev/null

echo "[smoke] execute_ic_tx"
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

TX_ID=$(EXEC_OUT="$EXEC_OUT" python - <<'PY'
import os, re, sys
text = os.environ.get("EXEC_OUT", "")
m = re.search(r'tx_id\s*=\s*blob\s*"([^"]*)"', text)
if not m:
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

echo "[smoke] submit_eth_tx -> produce_block"
ETH_TX_HEX="01"
ETH_TX_ID=$(dfx canister call evm_canister submit_eth_tx "(vec { $(python - <<PY
tx = bytes.fromhex("$ETH_TX_HEX")
print('; '.join(str(b) for b in tx))
PY
) })")

ETH_TX_ID_BYTES=$(ETH_TX_ID="$ETH_TX_ID" python - <<'PY'
import os, re, sys
text = os.environ.get("ETH_TX_ID", "")
m = re.search(r'blob\s*"([^"]*)"', text)
if not m:
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

dfx canister call evm_canister produce_block '(1)' >/dev/null
dfx canister call evm_canister get_receipt "(vec { $ETH_TX_ID_BYTES })" >/dev/null
