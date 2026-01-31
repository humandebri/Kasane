#!/usr/bin/env bash
# where: playground smoke harness
# what: replay the critical RPC/Tx path against the playground canister
# why: make the manual tests repeatable and capture cycle consumption
set -euo pipefail

CANISTER_ID="${CANISTER_ID:-mkv5r-3aaaa-aaaab-qabsq-cai}"
DFX="dfx --network playground"
USE_DEV_FAUCET="${USE_DEV_FAUCET:-0}"
DEV_FAUCET_AMOUNT="${DEV_FAUCET_AMOUNT:-1000000000000000000}"

cycle_balance() {
  local label=$1
  local balance
  balance=$($DFX canister call "$CANISTER_ID" get_cycle_balance --output json | tr -d '"')
  echo "$label cycle_balance=$balance"
  echo "$balance"
}

log() {
  echo "[playground-smoke] $*"
}

encode_raw_tx() {
  python - <<'PY'
from eth_keys import keys
from eth_utils import keccak
from rlp import encode
chain_id = 4801360
nonce = 0
gas_price = 1_000_000_000
gas_limit = 21_000
value = 1_000_000_000_000_000_000
recipient = bytes.fromhex('0000000000000000000000000000000000000001')
data = b''
private_key = keys.PrivateKey(b"\x01" * 32)
signing_hash = keccak(encode([nonce, gas_price, gas_limit, recipient, value, data, chain_id, 0, 0]))
sig = private_key.sign_msg_hash(signing_hash)
v = sig.v + 35 + 2 * chain_id
raw = encode([nonce, gas_price, gas_limit, recipient, value, data, v, sig.r, sig.s])
print('; '.join(str(b) for b in raw))
PY
}

raw_tx_sender_bytes() {
  python - <<'PY'
from eth_keys import keys
private_key = keys.PrivateKey(b"\x01" * 32)
addr = private_key.public_key.to_canonical_address()
print('; '.join(str(b) for b in addr))
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

assert_command() {
  bash -c "$1" >/dev/null
}

log "starting playground smoke"
before=$(cycle_balance "before")
log "triggering ic tx"
IC_BYTES=$(python - <<'PY'
version = b'\x02'
to = bytes.fromhex('0000000000000000000000000000000000000001')
value = (0).to_bytes(32, 'big')
gas = (500000).to_bytes(8, 'big')
nonce = (0).to_bytes(8, 'big')
max_fee = (2_000_000_000).to_bytes(16, 'big')
max_priority = (1_000_000_000).to_bytes(16, 'big')
data = b''
data_len = len(data).to_bytes(4, 'big')
tx = version + to + value + gas + nonce + max_fee + max_priority + data_len + data
print('; '.join(str(b) for b in tx))
PY
)
EXEC_OUT=$($DFX canister call $CANISTER_ID execute_ic_tx "(vec { $IC_BYTES })")
echo "$EXEC_OUT" | assert_ok_variant
if [[ "$USE_DEV_FAUCET" == "1" ]]; then
  log "dev_mint for eth sender"
  SENDER_BYTES=$(raw_tx_sender_bytes)
  assert_command "$DFX canister call $CANISTER_ID dev_mint \"(vec { $SENDER_BYTES }, $DEV_FAUCET_AMOUNT)\""
fi
log "submitting eth raw tx"
RAW_TX=$(encode_raw_tx)
SUBMIT_ETH_OUT=$($DFX canister call $CANISTER_ID submit_eth_tx "(vec { $RAW_TX })")
ETH_TX_ID_BYTES=$(echo "$SUBMIT_ETH_OUT" | parse_ok_blob_bytes)
log "producing block"
assert_command "$DFX canister call $CANISTER_ID produce_block '(1)'"
log "fetching receipt for eth tx"
$DFX canister call "$CANISTER_ID" get_receipt "(vec { $ETH_TX_ID_BYTES })"
after=$(cycle_balance "after")
delta=$((before - after))
log "cycles consumed delta=$delta"
log "fetching block1"
$DFX canister call "$CANISTER_ID" get_block '(1)'
log "fetching block2"
$DFX canister call "$CANISTER_ID" get_block '(2)'
log "playground smoke finished"
