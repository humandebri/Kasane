#!/usr/bin/env bash
# where: playground smoke harness
# what: replay the critical RPC/Tx path against the playground canister
# why: make the manual tests repeatable and capture cycle consumption
set -euo pipefail

CANISTER_ID="${CANISTER_ID:-mkv5r-3aaaa-aaaab-qabsq-cai}"
NETWORK="${NETWORK:-playground}"
FUNDED_ETH_PRIVKEY="${FUNDED_ETH_PRIVKEY:-}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SMOKE_ENV_FILE="${SMOKE_ENV_FILE:-${SCRIPT_DIR}/.playground_smoke.env}"
DFX=(dfx --network "${NETWORK}")

if [[ -z "${FUNDED_ETH_PRIVKEY}" && -f "${SMOKE_ENV_FILE}" ]]; then
  # shellcheck source=/dev/null
  source "${SMOKE_ENV_FILE}"
fi

cycle_balance() {
  local label=$1
  local raw
  local balance
  raw=$("${DFX[@]}" canister call --query "$CANISTER_ID" get_cycle_balance '()')
  if ! balance=$(python - "$raw" <<'PY'
import re
import sys
text = sys.argv[1]
m = re.search(r"(\d+)", text)
if not m:
    sys.stderr.write("[playground-smoke] failed to parse cycle balance\n")
    sys.stderr.write(text + "\n")
    sys.exit(1)
print(m.group(1))
PY
); then
    return 1
  fi
  echo "[playground-smoke] ${label} cycle_balance=${balance}" >&2
  echo "$balance"
}

log() {
  echo "[playground-smoke] $*"
}

raw_tx_bytes_with_nonce() {
  local nonce_val="$1"
  local privkey="$2"
  cargo run -q -p ic-evm-core --features local-signer-bin --bin eth_raw_tx -- \
    --mode raw \
    --privkey "$privkey" \
    --to "0000000000000000000000000000000000000001" \
    --value "1000000000000000000" \
    --gas-price "1000000000" \
    --gas-limit "21000" \
    --nonce "$nonce_val" \
    --chain-id "4801360"
}

raw_tx_sender_bytes() {
  cargo run -q -p ic-evm-core --features local-signer-bin --bin eth_raw_tx -- \
    --mode sender \
    --privkey "$1" \
    --to "0000000000000000000000000000000000000001" \
    --value "0" \
    --gas-price "1" \
    --gas-limit "21000" \
    --nonce "0" \
    --chain-id "4801360"
}

raw_tx_sender_blob() {
  local hex
  local privkey="$1"
  hex=$(cargo run -q -p ic-evm-core --features local-signer-bin --bin eth_raw_tx -- \
    --mode sender-hex \
    --privkey "$privkey" \
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

random_privkey() {
  cargo run -q -p ic-evm-core --features local-signer-bin --bin eth_raw_tx -- --mode genkey
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

query_block_number() {
  local out
  out=$("${DFX[@]}" canister call --query "$CANISTER_ID" rpc_eth_block_number '( )' 2>/dev/null || true)
  python - <<PY
import re
text = """${out}"""
m = re.search(r'(\d+)', text)
print(m.group(1) if m else "0")
PY
}

wait_for_head_advance() {
  local note="$1"
  local start deadline
  start="$(query_block_number)"
  deadline=$((SECONDS + 30))
  while [[ ${SECONDS} -lt ${deadline} ]]; do
    if [[ "$(query_block_number)" -gt "${start}" ]]; then
      return 0
    fi
    sleep 1
  done
  echo "[playground-smoke] auto-mine did not advance head: ${note}" >&2
  return 1
}

log "starting playground smoke"
if ! before=$(cycle_balance "before"); then
  exit 1
fi
CALLER_PRINCIPAL=$(dfx identity get-principal)
CALLER_HEX=$(cargo run -q -p ic-evm-core --bin derive_evm_address -- "$CALLER_PRINCIPAL")
CALLER_BLOB=$(python - <<PY
data = bytes.fromhex("$CALLER_HEX")
print(''.join(f'\\\\{b:02x}' for b in data))
PY
)
log "triggering ic tx"
EXEC_OUT=""
SELECTED_NONCE=""
EXPECTED_NONCE=$("${DFX[@]}" canister call --query "$CANISTER_ID" expected_nonce_by_address "(blob \"$CALLER_BLOB\")")
IC_NONCE=$(python - <<PY
import re
text = "$EXPECTED_NONCE"
m = re.search(r"(\\d+)", text)
print(m.group(1) if m else "0")
PY
)
for nonce_val in "$IC_NONCE"; do
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
  EXEC_OUT=$(icp canister call -n "${NETWORK}" "$CANISTER_ID" submit_ic_tx "(vec { $IC_BYTES })")
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
  echo "[playground-smoke] submit_ic_tx failed: $EXEC_OUT"
  exit 1
fi
log "submit_ic_tx accepted nonce=${SELECTED_NONCE}"
log "waiting auto-mine for ic tx"
wait_for_head_advance "ic tx inclusion"
SKIP_ETH=0
if [[ -n "$FUNDED_ETH_PRIVKEY" ]]; then
  ETH_PRIVKEY="$FUNDED_ETH_PRIVKEY"
  SENDER_BLOB=$(raw_tx_sender_blob "$ETH_PRIVKEY")
  log "using provided funded eth sender (FUNDED_ETH_PRIVKEY)"
else
  SKIP_ETH=1
  log "skipping eth raw tx smoke (provide FUNDED_ETH_PRIVKEY)"
fi
if [[ "$SKIP_ETH" == "0" ]]; then
  ETH_BALANCE_OUT=$("${DFX[@]}" canister call --query "$CANISTER_ID" rpc_eth_get_balance "(blob \"$SENDER_BLOB\")")
  ETH_BALANCE_WEI=$(BALANCE_TEXT="$ETH_BALANCE_OUT" python - <<'PY'
import os
import re
import sys

text = os.environ.get("BALANCE_TEXT", "")
match = re.search(r'variant\s*\{\s*(?:ok|Ok)\s*=\s*blob\s*"([^"]*)"', text)
if not match:
    sys.stderr.write("[playground-smoke] rpc_eth_get_balance did not return ok blob\n")
    sys.stderr.write(text + "\n")
    sys.exit(1)
escaped = match.group(1)
out = bytearray()
i = 0
while i < len(escaped):
    if escaped[i] == "\\":
        if i + 2 < len(escaped) and all(c in "0123456789abcdefABCDEF" for c in escaped[i + 1:i + 3]):
            out.append(int(escaped[i + 1:i + 3], 16))
            i += 3
            continue
        if i + 1 < len(escaped):
            out.append(ord(escaped[i + 1]))
            i += 2
            continue
    out.append(ord(escaped[i]))
    i += 1
print(int.from_bytes(out, "big"))
PY
)
  if ! ETH_BALANCE_WEI="$ETH_BALANCE_WEI" python - <<'PY'
import os
import sys
value = os.environ.get("ETH_BALANCE_WEI", "")
if not value:
    sys.exit(1)
sys.exit(0 if int(value) > 0 else 1)
PY
  then
    echo "[playground-smoke] eth sender balance is zero; set FUNDED_ETH_PRIVKEY with funded account" >&2
    exit 1
  fi
  log "eth sender funded balance_wei=${ETH_BALANCE_WEI}"

  log "submitting eth raw tx"
  SUBMIT_ETH_OUT=""
  ETH_TX_ID_BYTES=""
  EXPECTED_ETH_NONCE=$("${DFX[@]}" canister call --query "$CANISTER_ID" expected_nonce_by_address "(blob \"$SENDER_BLOB\")")
  ETH_NONCE=$(python - <<PY
import re
text = "$EXPECTED_ETH_NONCE"
m = re.search(r"(\\d+)", text)
print(m.group(1) if m else "0")
PY
)
  for nonce_val in "$ETH_NONCE"; do
    RAW_TX="$(raw_tx_bytes_with_nonce "$nonce_val" "$ETH_PRIVKEY")"
    SUBMIT_ETH_OUT=$(icp canister call -n "${NETWORK}" "$CANISTER_ID" rpc_eth_send_raw_transaction "(vec { $RAW_TX })")
    if echo "$SUBMIT_ETH_OUT" | assert_ok_variant; then
      ETH_TX_ID_BYTES=$(echo "$SUBMIT_ETH_OUT" | parse_ok_blob_bytes)
      break
    fi
    echo "[playground-smoke] rpc_eth_send_raw_transaction failed: $SUBMIT_ETH_OUT"
    exit 1
  done
  if [[ -z "$ETH_TX_ID_BYTES" ]]; then
    echo "[playground-smoke] rpc_eth_send_raw_transaction failed: $SUBMIT_ETH_OUT"
    exit 1
  fi
  log "waiting auto-mine for eth tx"
  wait_for_head_advance "eth tx inclusion"
  log "fetching receipt for eth tx"
  "${DFX[@]}" canister call --query "$CANISTER_ID" get_receipt "(vec { $ETH_TX_ID_BYTES })"
fi
if ! after=$(cycle_balance "after"); then
  exit 1
fi
delta=$((before - after))
log "cycles consumed delta=$delta"
log "fetching block1"
"${DFX[@]}" canister call --query "$CANISTER_ID" get_block '(1)'
if [[ "$SKIP_ETH" == "0" ]]; then
  log "fetching block2"
  "${DFX[@]}" canister call --query "$CANISTER_ID" get_block '(2)'
fi
log "playground smoke finished"
