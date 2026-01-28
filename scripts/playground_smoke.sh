#!/usr/bin/env bash
# where: playground smoke harness
# what: replay the critical RPC/Tx path against the playground canister
# why: make the manual tests repeatable and capture cycle consumption
set -euo pipefail

CANISTER_ID="${CANISTER_ID:-mkv5r-3aaaa-aaaab-qabsq-cai}"
DFX="dfx --network playground"

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

assert_command() {
  bash -c "$1" >/dev/null
}

log "starting playground smoke"
before=$(cycle_balance "before")
log "triggering ic tx"
assert_command "$DFX canister call $CANISTER_ID execute_ic_tx \"(vec { 1; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 1; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 7; 161; 32; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0 }\")"
log "submitting eth raw tx"
RAW_TX=$(encode_raw_tx)
assert_command "$DFX canister call $CANISTER_ID submit_eth_tx \"(vec { $RAW_TX })\""
log "producing block"
assert_command "$DFX canister call $CANISTER_ID produce_block '(1)'"
after=$(cycle_balance "after")
delta=$((before - after))
log "cycles consumed delta=$delta"
log "fetching block1"
$DFX canister call "$CANISTER_ID" get_block '(1)'
log "fetching block2"
$DFX canister call "$CANISTER_ID" get_block '(2)'
log "playground smoke finished"
