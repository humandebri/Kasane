#!/usr/bin/env bash
# where: playground smoke harness
# what: replay the critical RPC/Tx path against the playground canister
# why: make the manual tests repeatable and capture cycle consumption
set -euo pipefail

CANISTER_ID="${CANISTER_ID:-mkv5r-3aaaa-aaaab-qabsq-cai}"
DFX="dfx canister --network playground"
USE_DEV_FAUCET="${USE_DEV_FAUCET:-0}"
DEV_FAUCET_AMOUNT="${DEV_FAUCET_AMOUNT:-1000000000000000000}"

cycle_balance() {
  local label=$1
  local balance
  balance=$($DFX call "$CANISTER_ID" get_cycle_balance --output json | tr -d '"_' )
  echo "[playground-smoke] ${label} cycle_balance=${balance}" >&2
  echo "$balance"
}

log() {
  echo "[playground-smoke] $*"
}

raw_tx_bytes_with_nonce() {
  local nonce_val="$1"
  cargo run -q -p ic-evm-core --bin eth_raw_tx -- \
    --mode raw \
    --privkey "0101010101010101010101010101010101010101010101010101010101010101" \
    --to "0000000000000000000000000000000000000001" \
    --value "1000000000000000000" \
    --gas-price "1000000000" \
    --gas-limit "21000" \
    --nonce "$nonce_val" \
    --chain-id "4801360"
}

raw_tx_sender_bytes() {
  cargo run -q -p ic-evm-core --bin eth_raw_tx -- \
    --mode sender \
    --privkey "0101010101010101010101010101010101010101010101010101010101010101" \
    --to "0000000000000000000000000000000000000001" \
    --value "0" \
    --gas-price "1" \
    --gas-limit "21000" \
    --nonce "0" \
    --chain-id "4801360"
}

raw_tx_sender_blob() {
  local hex
  hex=$(cargo run -q -p ic-evm-core --bin eth_raw_tx -- \
    --mode sender-hex \
    --privkey "0101010101010101010101010101010101010101010101010101010101010101" \
    --to "0000000000000000000000000000000000000001" \
    --value "0" \
    --gas-price "1" \
    --gas-limit "21000" \
    --nonce "0" \
    --chain-id "4801360")
  python - <<PY
data = bytes.fromhex("$hex")
print(''.join(f'\\\\{b:02x}' for b in data))
PY
}

parse_ok_blob_bytes() {
  python - <<'PY'
import re, sys
text = sys.stdin.read()
m = re.search(r'variant\s*\{\s*(?:ok|Ok)\s*=\s*blob\s*\"([^\"]*)\"', text)
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
}

assert_ok_variant() {
  python - <<'PY'
import re, sys
text = sys.stdin.read()
if not re.search(r'variant\s*\{\s*(?:ok|Ok)\s*=', text):
    sys.exit(1)
PY
}

is_ok_variant() {
  python - <<'PY'
import os, re, sys
text = os.environ.get("EXEC_OUT", "")
sys.exit(0 if re.search(r'variant\s*\{\s*(?:ok|Ok)\s*=', text) else 1)
PY
}

assert_command() {
  bash -c "$1" >/dev/null
}

log "starting playground smoke"
before=$(cycle_balance "before")
if [[ "$USE_DEV_FAUCET" == "1" ]]; then
  log "dev_mint for ic caller"
  CALLER_PRINCIPAL=$(dfx identity get-principal)
  CALLER_HEX=$(cargo run -q -p ic-evm-core --bin caller_evm -- "$CALLER_PRINCIPAL")
  CALLER_BLOB=$(python - <<PY
data = bytes.fromhex("$CALLER_HEX")
print(''.join(f'\\\\{b:02x}' for b in data))
PY
)
  assert_command "$DFX call $CANISTER_ID dev_mint \"(blob \\\"$CALLER_BLOB\\\", $DEV_FAUCET_AMOUNT:nat)\""
fi
log "triggering ic tx"
EXEC_OUT=""
SELECTED_NONCE=""
for nonce_val in $(seq 0 50); do
  IC_BYTES=$(python - <<PY
version = b'\x02'
to = bytes.fromhex('0000000000000000000000000000000000000001')
value = (0).to_bytes(32, 'big')
gas = (500000).to_bytes(8, 'big')
nonce = (${nonce_val}).to_bytes(8, 'big')
max_fee = (2_000_000_000).to_bytes(16, 'big')
max_priority = (1_000_000_000).to_bytes(16, 'big')
data = b''
try:
    import time
    data = int(time.time()).to_bytes(8, 'big')
except Exception:
    data = b'\x01'
data_len = len(data).to_bytes(4, 'big')
tx = version + to + value + gas + nonce + max_fee + max_priority + data_len + data
print('; '.join(str(b) for b in tx))
PY
)
  EXEC_OUT=$($DFX call $CANISTER_ID execute_ic_tx "(vec { $IC_BYTES })")
  if EXEC_OUT="$EXEC_OUT" is_ok_variant; then
    SELECTED_NONCE="$nonce_val"
    break
  fi
  if echo "$EXEC_OUT" | grep -qi "nonce too low"; then
    continue
  fi
  break
done
if ! EXEC_OUT="$EXEC_OUT" is_ok_variant; then
  echo "[playground-smoke] execute_ic_tx failed: $EXEC_OUT"
  exit 1
fi
log "execute_ic_tx accepted nonce=${SELECTED_NONCE}"
SKIP_ETH=0
RAW_TX="$(raw_tx_bytes_with_nonce 0)"
if [[ "$USE_DEV_FAUCET" == "1" ]]; then
  log "dev_mint for eth sender"
  SENDER_BLOB=$(raw_tx_sender_blob)
  assert_command "$DFX call $CANISTER_ID dev_mint \"(blob \\\"$SENDER_BLOB\\\", $DEV_FAUCET_AMOUNT:nat)\""
fi
log "submitting eth raw tx"
SUBMIT_ETH_OUT=""
ETH_TX_ID_BYTES=""
for nonce_val in $(seq 0 50); do
  RAW_TX="$(raw_tx_bytes_with_nonce "$nonce_val")"
  SUBMIT_ETH_OUT=$($DFX call $CANISTER_ID submit_eth_tx "(vec { $RAW_TX })")
  if echo "$SUBMIT_ETH_OUT" | assert_ok_variant; then
    ETH_TX_ID_BYTES=$(echo "$SUBMIT_ETH_OUT" | parse_ok_blob_bytes)
    break
  fi
  if echo "$SUBMIT_ETH_OUT" | grep -qi "nonce too low"; then
    continue
  fi
  if echo "$SUBMIT_ETH_OUT" | grep -qi "nonce gap"; then
    continue
  fi
  if echo "$SUBMIT_ETH_OUT" | grep -qi "tx already seen"; then
    continue
  fi
  echo "[playground-smoke] submit_eth_tx failed: $SUBMIT_ETH_OUT"
  exit 1
done
if [[ -z "$ETH_TX_ID_BYTES" ]]; then
  echo "[playground-smoke] submit_eth_tx failed: $SUBMIT_ETH_OUT"
  exit 1
fi
log "producing block"
assert_command "$DFX call $CANISTER_ID produce_block '(1)'"
log "fetching receipt for eth tx"
$DFX call "$CANISTER_ID" get_receipt "(vec { $ETH_TX_ID_BYTES })"
after=$(cycle_balance "after")
delta=$((before - after))
log "cycles consumed delta=$delta"
log "fetching block1"
$DFX call "$CANISTER_ID" get_block '(1)'
log "fetching block2"
$DFX call "$CANISTER_ID" get_block '(2)'
log "playground smoke finished"
