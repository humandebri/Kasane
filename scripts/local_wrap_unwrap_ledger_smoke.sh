#!/usr/bin/env bash
# where: local managed network smoke
# what: official ICRC ledger を deploy して wrap/unwrap を end-to-end で確認する
# why: dummy ledger では見えない fee pull / withdraw / unwrap transfer を local で再現するため
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
NETWORK="${NETWORK:-local}"
ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME:-}"
LEDGER_CACHE_DIR="${LEDGER_CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/kasane-local-ledger}"
LEDGER_RELEASE="${LEDGER_RELEASE:-ledger-suite-icrc-2026-03-09}"
LEDGER_WASM_GZ="${LEDGER_CACHE_DIR}/ic-icrc1-ledger.wasm.gz"
LEDGER_DID="${LEDGER_CACHE_DIR}/ledger.did"
LEDGER_WASM="${LEDGER_CACHE_DIR}/ic-icrc1-ledger.wasm"
WRAP_AMOUNT="${WRAP_AMOUNT:-1000000}"
WRAP_GAS_LIMIT="${WRAP_GAS_LIMIT:-800000}"
WRAP_EVM_NONCE="${WRAP_EVM_NONCE:-0}"
LEDGER_DECIMALS="${LEDGER_DECIMALS:-8}"
CONTRACTS_DIR="${ROOT_DIR}/tools/wrapper/contracts"
FACTORY_ARTIFACT_PATH="${CONTRACTS_DIR}/out/WrapTokenFactory.sol/WrapTokenFactory.json"
FACTORY_DEPLOY_GAS_LIMIT="${FACTORY_DEPLOY_GAS_LIMIT:-3000000}"
FACTORY_DEPLOY_FEE_BUMP="${FACTORY_DEPLOY_FEE_BUMP:-$(python - <<'PY'
import time
print(int(time.time_ns()) & 0xffff)
PY
)}"
UNWRAP_AMOUNT="${UNWRAP_AMOUNT:-1000000}"
UNWRAP_GAS_LIMIT="${UNWRAP_GAS_LIMIT:-300000}"
UNWRAP_USER_NONCE="${UNWRAP_USER_NONCE:-1}"
UNWRAP_DEADLINE="${UNWRAP_DEADLINE:-18446744073709551615}"
LEDGER_INITIAL_USER_BALANCE="${LEDGER_INITIAL_USER_BALANCE:-5000000000}"
LEDGER_INITIAL_WRAP_BALANCE="${LEDGER_INITIAL_WRAP_BALANCE:-5000000000}"
LEDGER_APPROVE_AMOUNT="${LEDGER_APPROVE_AMOUNT:-5000000000}"
LEDGER_TRANSFER_FEE="${LEDGER_TRANSFER_FEE:-10}"
CYCLE_FEE_E8S="${CYCLE_FEE_E8S:-1000000}"
GAS_PRICE_BUFFER_BPS="${GAS_PRICE_BUFFER_BPS:-12000}"
GENESIS_BALANCE_WEI="${GENESIS_BALANCE_WEI:-1000000000000000000}"
FACTORY_DEPLOY_MAX_FEE_BASE="${FACTORY_DEPLOY_MAX_FEE_BASE:-600000000000}"
UNWRAP_MAX_FEE_PER_GAS="${UNWRAP_MAX_FEE_PER_GAS:-600000000000}"
WAIT_RETRIES="${WAIT_RETRIES:-40}"
WAIT_SECONDS="${WAIT_SECONDS:-2}"
SKIP_BUILD="${SKIP_BUILD:-0}"
WRAPPER_DIR="${ROOT_DIR}/tools/wrapper"
TSX_BIN="${WRAPPER_DIR}/node_modules/.bin/tsx"
HELPER_TS="$(mktemp "${WRAPPER_DIR}/.kasane-local-ledger-helper.XXXXXX").mts"
GATEWAY_WASM="${ROOT_DIR}/target/wasm32-unknown-unknown/release/ic_evm_gateway.wasm"
WRAP_WASM="${ROOT_DIR}/target/wasm32-unknown-unknown/release/wrap_canister.wasm"

cleanup() {
  rm -f "${HELPER_TS}"
}
trap cleanup EXIT

log() {
  echo "[local-wrap-unwrap-ledger] $*"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "[local-wrap-unwrap-ledger] missing command: $1" >&2
    exit 1
  }
}

require_cmd cargo
require_cmd cast
require_cmd curl
require_cmd dfx
require_cmd didc
require_cmd forge
require_cmd gzip
require_cmd icp
require_cmd node
require_cmd npm
require_cmd python

FACTORY_DEPLOY_MAX_FEE="$((FACTORY_DEPLOY_MAX_FEE_BASE + FACTORY_DEPLOY_FEE_BUMP))"
FACTORY_DEPLOY_MAX_PRIORITY="$((300000000000 + FACTORY_DEPLOY_FEE_BUMP))"
WRAP_MINT_MAX_FEE_PER_GAS="$(
  python - <<PY
deploy_fee = int("${FACTORY_DEPLOY_MAX_FEE}")
unwrap_fee = int("${UNWRAP_MAX_FEE_PER_GAS}")
buffer_bps = int("${GAS_PRICE_BUFFER_BPS}")
reference_fee = max(deploy_fee, unwrap_fee)
numerator = reference_fee * buffer_bps
print((numerator + 9_999) // 10_000)
PY
)"
MIN_REQUIRED_GENESIS_WEI="$(
  python - <<PY
wrap_gas_limit = int("${WRAP_GAS_LIMIT}")
unwrap_gas_limit = int("${UNWRAP_GAS_LIMIT}")
factory_gas_limit = int("${FACTORY_DEPLOY_GAS_LIMIT}")
factory_max_fee = int("${FACTORY_DEPLOY_MAX_FEE}")
unwrap_max_fee = int("${UNWRAP_MAX_FEE_PER_GAS}")
wrap_max_fee = int("${WRAP_MINT_MAX_FEE_PER_GAS}")
caller_required = factory_gas_limit * factory_max_fee + unwrap_gas_limit * unwrap_max_fee
wrap_required = wrap_gas_limit * wrap_max_fee
# 同じ GENESIS_BALANCE_WEI を caller と wrap canister の両 sender に配るので、
# それぞれが自分の tx を前払いできる最小額を比較して大きい方に合わせる。
# 2 倍余白は local smoke の fee 変動や初回 token deploy 分の揺れを吸収するために維持する。
print(max(caller_required, wrap_required) * 2)
PY
)"
if [[ "${GENESIS_BALANCE_WEI}" -lt "${MIN_REQUIRED_GENESIS_WEI}" ]]; then
  log "bump genesis balance to cover local smoke max upfront gas cost"
  GENESIS_BALANCE_WEI="${MIN_REQUIRED_GENESIS_WEI}"
fi

mkdir -p "${LEDGER_CACHE_DIR}"

resolve_identity() {
  if [[ -n "${ICP_IDENTITY_NAME}" ]]; then
    echo "${ICP_IDENTITY_NAME}"
    return
  fi
  local current
  current="$(icp identity default 2>/dev/null || true)"
  if [[ -n "${current}" && "${current}" != "anonymous" ]]; then
    echo "${current}"
    return
  fi
  if icp identity list | awk '{print $1}' | grep -qx "ci-local"; then
    echo "ci-local"
    return
  fi
  icp identity list | awk '$1 != "*" {print $1} $1 == "*" {print $2}' | grep -v '^anonymous$' | head -n 1
}

ICP_IDENTITY_NAME="$(resolve_identity)"
if [[ -z "${ICP_IDENTITY_NAME}" || "${ICP_IDENTITY_NAME}" == "anonymous" ]]; then
  echo "[local-wrap-unwrap-ledger] no usable non-anonymous icp identity found" >&2
  exit 1
fi

if [[ ! -d "${WRAPPER_DIR}/node_modules" ]]; then
  log "npm ci (${WRAPPER_DIR})"
  (cd "${WRAPPER_DIR}" && npm ci)
fi

cat > "${HELPER_TS}" <<'TS'
import { Principal } from "@dfinity/principal";
import {
  WRAP_PRECOMPILE_ADDRESS,
  decimalToBytes32,
  deriveRequestId,
  deriveWrapRequestId,
  toSubmitIcTxData,
} from "/Users/0xhude/Desktop/ICP/Kasane/tools/wrapper/lib/request-id.ts";

function toVec(bytes: Uint8Array): string {
  return `vec { ${Array.from(bytes).join("; ")} }`;
}

const mode = process.argv[2];
if (!mode) {
  throw new Error("mode required");
}

if (mode === "principal-vec") {
  console.log(toVec(Principal.fromText(process.argv[3] ?? "").toUint8Array()));
} else if (mode === "principal-hex") {
  console.log(Buffer.from(Principal.fromText(process.argv[3] ?? "").toUint8Array()).toString("hex"));
} else if (mode === "wrap-request-id-vec") {
  const owner = Principal.fromText(process.argv[3] ?? "").toUint8Array();
  const assetId = Principal.fromText(process.argv[4] ?? "").toUint8Array();
  const amount = decimalToBytes32(process.argv[5] ?? "");
  const evmRecipient = Uint8Array.from(Buffer.from((process.argv[6] ?? "").replace(/^0x/, ""), "hex"));
  const evmNonce = BigInt(process.argv[7] ?? "0");
  const gasLimit = BigInt(process.argv[8] ?? "0");
  console.log(
    toVec(
      deriveWrapRequestId({
        fromOwner: owner,
        assetId,
        amount,
        evmRecipient,
        evmNonce,
        gasLimit,
      }),
    ),
  );
} else if (mode === "unwrap-json") {
  const callerPrincipal = process.argv[3] ?? "";
  const vaultCanisterId = process.argv[4] ?? "";
  const assetId = process.argv[5] ?? "";
  const amount = BigInt(process.argv[6] ?? "0");
  const recipient = process.argv[7] ?? "";
  const userNonce = BigInt(process.argv[8] ?? "0");
  const deadline = BigInt(process.argv[9] ?? "0");
  const callerHex = process.argv[10] ?? "";
  const callerEvmAddress = Uint8Array.from(Buffer.from(callerHex.replace(/^0x/, ""), "hex"));
  const data = toSubmitIcTxData({
    vaultCanisterId,
    assetId,
    amount,
    recipient,
    userNonce,
    deadline,
  });
  const requestId = deriveRequestId({
    callerEvmAddress,
    vaultCanisterId,
    assetId,
    amount,
    recipient,
    userNonce,
    deadline,
  });
  process.stdout.write(
    JSON.stringify({
      dataVec: toVec(data),
      requestIdVec: toVec(requestId),
      precompileHex: `0x${Buffer.from(WRAP_PRECOMPILE_ADDRESS).toString("hex")}`,
    }),
  );
} else {
  throw new Error(`unknown mode: ${mode}`);
}
TS

tsx_eval() {
  (cd "${WRAPPER_DIR}" && "${TSX_BIN}" "${HELPER_TS}" "$@")
}

download_ledger_artifacts() {
  if [[ -f "${LEDGER_WASM_GZ}" && -f "${LEDGER_DID}" ]]; then
    return
  fi
  local release_tag="${LEDGER_RELEASE}"
  if [[ "${release_tag}" == "latest" ]]; then
    release_tag="$(
      python - <<'PY'
import json
import urllib.request

page = 1
while page <= 10:
    url = f"https://api.github.com/repos/dfinity/ic/releases?per_page=100&page={page}"
    with urllib.request.urlopen(url) as resp:
        data = json.load(resp)
    for item in data:
        tag_name = item.get("tag_name", "")
        if isinstance(tag_name, str) and tag_name.startswith("ledger-suite-icrc"):
            print(tag_name)
            raise SystemExit(0)
    page += 1
raise SystemExit("ledger-suite-icrc release not found")
PY
    )"
  fi
  log "download official ledger artifacts: ${release_tag}"
  curl -L --fail --output "${LEDGER_DID}" "https://github.com/dfinity/ic/releases/download/${release_tag}/ledger.did"
  curl -L --fail --output "${LEDGER_WASM_GZ}" "https://github.com/dfinity/ic/releases/download/${release_tag}/ic-icrc1-ledger.wasm.gz"
}

decompress_ledger_wasm() {
  gzip -dc "${LEDGER_WASM_GZ}" > "${LEDGER_WASM}"
}

icp_call() {
  icp canister call -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" "$@"
}

query_pp() {
  local did_file="$1"
  local canister="$2"
  local method="$3"
  local args="$4"
  local encoded
  local hex
  encoded="$(didc encode -d "${did_file}" -m "${method}" "${args}")"
  hex="$(icp canister call --query -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --args-format hex -o hex "${canister}" "${method}" "${encoded}")"
  didc decode -f hex -d "${did_file}" -m "${method}" "${hex}" | python -c 'import sys; print(" ".join(sys.stdin.read().split()))'
}

update_call() {
  local did_file="$1"
  local canister="$2"
  local method="$3"
  local args="$4"
  local encoded
  local hex
  encoded="$(didc encode -d "${did_file}" -m "${method}" "${args}")"
  hex="$(icp canister call -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --args-format hex -o hex "${canister}" "${method}" "${encoded}")"
  didc decode -f hex -d "${did_file}" -m "${method}" "${hex}" | python -c 'import sys; print(" ".join(sys.stdin.read().split()))'
}

hex_to_candid_vec() {
  python - "$1" <<'PY'
import sys
hexv = sys.argv[1].strip().lower()
if hexv.startswith("0x"):
    hexv = hexv[2:]
raw = bytes.fromhex(hexv)
print("vec { " + "; ".join(str(b) for b in raw) + " }")
PY
}

extract_result_ok_blob_hex() {
  python -c 'import re, sys
text = sys.stdin.read()
vec_match = re.search(r"Ok = vec \{([^}]*)\}", text)
if vec_match:
    nums = [part.strip() for part in vec_match.group(1).split(";") if part.strip()]
    print(bytes(int(num) for num in nums).hex())
    raise SystemExit(0)
blob_match = re.search(r"Ok = blob \"([^\"]*)\"", text)
if not blob_match:
    print(text, file=sys.stderr)
    raise SystemExit("ok blob not found")
blob = blob_match.group(1)
out = bytearray()
i = 0
while i < len(blob):
    if blob[i] == "\\":
        out.append(int(blob[i + 1:i + 3], 16))
        i += 3
    else:
        out.append(ord(blob[i]))
        i += 1
print(out.hex())'
}

extract_named_blob_hex() {
  local field="$1"
  python -c 'import re, sys
field = sys.argv[1]
text = sys.stdin.read()
match = re.search(rf"{re.escape(field)} = blob \"([^\"]*)\"", text)
if not match:
    raise SystemExit(f"{field} blob not found")
blob = match.group(1)
out = bytearray()
i = 0
while i < len(blob):
    if blob[i] == "\\":
        out.append(int(blob[i + 1:i + 3], 16))
        i += 3
    else:
        out.append(ord(blob[i]))
        i += 1
print(out.hex())' "$field"
}

extract_named_opt_blob_hex() {
  local field="$1"
  python -c 'import re, sys
field = sys.argv[1]
text = sys.stdin.read()
match = re.search(rf"{re.escape(field)} = opt blob \"([^\"]*)\"", text)
if not match:
    raise SystemExit(f"{field} opt blob not found")
blob = match.group(1)
out = bytearray()
i = 0
while i < len(blob):
    if blob[i] == "\\":
        out.append(int(blob[i + 1:i + 3], 16))
        i += 3
    else:
        out.append(ord(blob[i]))
        i += 1
print(out.hex())' "$field"
}

u256_hex_to_address_hex() {
  python -c 'import sys
hexv = sys.stdin.read().strip().lower()
if hexv.startswith("0x"):
    hexv = hexv[2:]
print(hexv[-40:])'
}

u256_hex_to_decimal() {
  python -c 'import sys
hexv = sys.stdin.read().strip().lower()
if hexv.startswith("0x"):
    hexv = hexv[2:]
print(int(hexv or "0", 16))'
}

rpc_eth_call_hex() {
  local to_hex="$1"
  local data_hex="$2"
  query_pp "${ROOT_DIR}/crates/ic-evm-gateway/evm_canister.did" evm_canister rpc_eth_call_object "(record {
    to = opt $(hex_to_candid_vec "${to_hex}");
    from = opt $(hex_to_candid_vec "${CALLER_EVM_HEX}");
    gas = opt 500000 : opt nat64;
    gas_price = null;
    nonce = null;
    max_fee_per_gas = null;
    max_priority_fee_per_gas = null;
    chain_id = null;
    tx_type = null;
    access_list = null;
    value = null;
    data = opt $(hex_to_candid_vec "${data_hex}");
  })"
}

wait_for_pattern() {
  local label="$1"
  local command="$2"
  local pattern="$3"
  local last=""
  for _ in $(seq 1 "${WAIT_RETRIES}"); do
    last="$(eval "${command}" 2>&1 || true)"
    if [[ "${last}" == *"${pattern}"* ]]; then
      printf '%s' "${last}"
      return 0
    fi
    sleep "${WAIT_SECONDS}"
  done
  echo "[local-wrap-unwrap-ledger] ${label} did not reach pattern: ${pattern}" >&2
  echo "${last}" >&2
  return 1
}

retry_command_output() {
  local label="$1"
  shift
  local last=""
  for _ in $(seq 1 "${WAIT_RETRIES}"); do
    if last="$("$@" 2>&1)"; then
      printf '%s' "${last}"
      return 0
    fi
    sleep "${WAIT_SECONDS}"
  done
  echo "[local-wrap-unwrap-ledger] ${label} did not succeed" >&2
  echo "${last}" >&2
  return 1
}

assert_local_port_available() {
  python - <<'PY'
import socket
sock = socket.socket()
sock.settimeout(0.5)
busy = sock.connect_ex(("127.0.0.1", 8000)) == 0
sock.close()
if busy:
    raise SystemExit(1)
PY
}

log "clean start local managed network"
icp network stop "${NETWORK}" >/dev/null 2>&1 || true
if ! assert_local_port_available; then
  echo "[local-wrap-unwrap-ledger] port 8000 is still in use after 'icp network stop ${NETWORK}'" >&2
  echo "[local-wrap-unwrap-ledger] stop stale pocket-ic/replica processes before running this smoke" >&2
  exit 1
fi
icp network start "${NETWORK}" -d >/dev/null 2>&1 &

download_ledger_artifacts
decompress_ledger_wasm

log "build gateway and wrap wasm"
if [[ "${SKIP_BUILD}" == "1" ]]; then
  [[ -f "${GATEWAY_WASM}" ]] || { echo "[local-wrap-unwrap-ledger] missing wasm: ${GATEWAY_WASM}" >&2; exit 1; }
  [[ -f "${WRAP_WASM}" ]] || { echo "[local-wrap-unwrap-ledger] missing wasm: ${WRAP_WASM}" >&2; exit 1; }
  log "skip build and reuse existing release wasm"
else
  cargo build --release --target wasm32-unknown-unknown -p ic-evm-gateway -p wrap-canister
fi
log "build solidity factory contracts"
(cd "${CONTRACTS_DIR}" && forge build >/dev/null)

log "create canisters"
LEDGER_CANISTER_ID="$(retry_command_output "create detached ledger canister" icp canister create -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --detached -q)"
retry_command_output "create evm_canister" icp canister create -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" evm_canister >/dev/null
retry_command_output "create wrap_canister" icp canister create -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" wrap_canister >/dev/null
EVM_CANISTER_ID="$(retry_command_output "resolve evm canister id" icp canister status -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --id-only evm_canister)"
WRAP_CANISTER_ID="$(retry_command_output "resolve wrap canister id" icp canister status -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --id-only wrap_canister)"
CALLER_PRINCIPAL="$(icp identity principal --identity "${ICP_IDENTITY_NAME}")"
LEDGER_MINTER_PRINCIPAL="${LEDGER_MINTER_PRINCIPAL:-${EVM_CANISTER_ID}}"
CALLER_EVM_HEX="$(cargo run -q -p ic-evm-core --bin derive_evm_address -- "${CALLER_PRINCIPAL}")"
WRAP_CANISTER_EVM_HEX="$(cargo run -q -p ic-evm-core --bin derive_evm_address -- "${WRAP_CANISTER_ID}")"
ASSET_ID_HEX="$(tsx_eval principal-hex "${LEDGER_CANISTER_ID}")"
LEDGER_INIT_ARGS="(variant { Init = record {
  token_symbol = \"LICP\";
  token_name = \"Local ICP\";
  minting_account = record { owner = principal \"${LEDGER_MINTER_PRINCIPAL}\"; subaccount = null; };
  fee_collector_account = null;
  transfer_fee = ${LEDGER_TRANSFER_FEE};
  decimals = opt ${LEDGER_DECIMALS};
  max_memo_length = null;
  metadata = vec {};
  feature_flags = opt record { icrc2 = true };
  initial_balances = vec {
    record { record { owner = principal \"${CALLER_PRINCIPAL}\"; subaccount = null; }; ${LEDGER_INITIAL_USER_BALANCE} };
    record { record { owner = principal \"${WRAP_CANISTER_ID}\"; subaccount = null; }; ${LEDGER_INITIAL_WRAP_BALANCE} };
  };
  archive_options = record {
    num_blocks_to_archive = 1000 : nat64;
    max_transactions_per_response = null;
    trigger_threshold = 2000 : nat64;
    max_message_size_bytes = null;
    controller_id = principal \"${CALLER_PRINCIPAL}\";
    cycles_for_archive_creation = opt 10000000000000 : opt nat64;
    node_max_memory_size_bytes = null;
    more_controller_ids = null;
  };
  index_principal = null;
} })"
LEDGER_INIT_ARGS_HEX="$(
  didc encode \
    -d "${LEDGER_DID}" \
    -t '(LedgerArg)' \
    "${LEDGER_INIT_ARGS}"
)"
GATEWAY_INIT_ARGS="(opt record {
  genesis_balances = vec {
    record { address = vec { $(python - <<PY
hexv = "${CALLER_EVM_HEX}".strip()
print("; ".join(str(b) for b in bytes.fromhex(hexv)))
PY
) }; amount = ${GENESIS_BALANCE_WEI} : nat; };
    record { address = vec { $(python - <<PY
hexv = "${WRAP_CANISTER_EVM_HEX}".strip()
print("; ".join(str(b) for b in bytes.fromhex(hexv)))
PY
) }; amount = ${GENESIS_BALANCE_WEI} : nat; };
  };
  wrap_canister_id = principal \"${WRAP_CANISTER_ID}\";
})"
GATEWAY_INIT_ARGS_HEX="$(
  didc encode \
    -d "${ROOT_DIR}/crates/ic-evm-gateway/evm_canister.did" \
    -t '(opt InitArgs)' \
    "${GATEWAY_INIT_ARGS}"
)"
log "install ledger"
icp canister install "${LEDGER_CANISTER_ID}" -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --mode install --wasm "${LEDGER_WASM}" --args-format hex --args "${LEDGER_INIT_ARGS_HEX}" >/dev/null

log "install gateway"
icp canister install evm_canister -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --mode reinstall --wasm "${GATEWAY_WASM}" --args-format hex --args "${GATEWAY_INIT_ARGS_HEX}" >/dev/null

FACTORY_CREATION_HEX="$(
  FACTORY_ARTIFACT_PATH="${FACTORY_ARTIFACT_PATH}" python - <<'PY'
import json
import os
from pathlib import Path
artifact = Path(os.environ["FACTORY_ARTIFACT_PATH"])
print(json.loads(artifact.read_text())["bytecode"]["object"].removeprefix("0x"))
PY
)"
FACTORY_CONSTRUCTOR_HEX="$(cd "${CONTRACTS_DIR}" && cast abi-encode "constructor(address)" "0x${WRAP_CANISTER_EVM_HEX}")"
FACTORY_DEPLOY_DATA_HEX="${FACTORY_CREATION_HEX}${FACTORY_CONSTRUCTOR_HEX#0x}"
log "deploy wrap token factory"
FACTORY_DEPLOY_ARGS="(record {
  to = null;
  value = 0 : nat;
  gas_limit = ${FACTORY_DEPLOY_GAS_LIMIT} : nat64;
  nonce = 0 : nat64;
  max_fee_per_gas = ${FACTORY_DEPLOY_MAX_FEE} : nat;
  max_priority_fee_per_gas = ${FACTORY_DEPLOY_MAX_PRIORITY} : nat;
  data = $(hex_to_candid_vec "${FACTORY_DEPLOY_DATA_HEX}");
})"
FACTORY_DEPLOY_ARGS_HEX="$(didc encode -d "${ROOT_DIR}/crates/ic-evm-gateway/evm_canister.did" -m submit_ic_tx "${FACTORY_DEPLOY_ARGS}")"
FACTORY_DEPLOY_REPLY_HEX="$(icp canister call -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --args-format hex -o hex evm_canister submit_ic_tx "${FACTORY_DEPLOY_ARGS_HEX}")"
FACTORY_DEPLOY_OUT="$(didc decode -f hex -d "${ROOT_DIR}/crates/ic-evm-gateway/evm_canister.did" -m submit_ic_tx "${FACTORY_DEPLOY_REPLY_HEX}" | python -c 'import sys; print(" ".join(sys.stdin.read().split()))')"
[[ "${FACTORY_DEPLOY_OUT}" != *"variant { Err ="* ]] || { echo "${FACTORY_DEPLOY_OUT}" >&2; exit 1; }
FACTORY_DEPLOY_TX_ID_HEX="$(printf '%s' "${FACTORY_DEPLOY_OUT}" | extract_result_ok_blob_hex)"
FACTORY_RECEIPT_CMD="query_pp '${ROOT_DIR}/crates/ic-evm-gateway/evm_canister.did' evm_canister rpc_eth_get_transaction_receipt_with_status_by_tx_id '( $(hex_to_candid_vec "${FACTORY_DEPLOY_TX_ID_HEX}") )'"
FACTORY_RECEIPT_OUT="$(wait_for_pattern "factory deploy receipt" "${FACTORY_RECEIPT_CMD}" "contract_address = opt blob")"
[[ "${FACTORY_RECEIPT_OUT}" == *"status = 1 : nat8"* ]] || { echo "${FACTORY_RECEIPT_OUT}" >&2; exit 1; }
FACTORY_ADDRESS_HEX="$(printf '%s' "${FACTORY_RECEIPT_OUT}" | extract_named_opt_blob_hex "contract_address")"

WRAP_INIT_ARGS="(record {
  kasane_canister = principal \"${EVM_CANISTER_ID}\";
  evm_gateway_canister = principal \"${EVM_CANISTER_ID}\";
  fee_ledger_canister = principal \"${LEDGER_CANISTER_ID}\";
  cycle_fee_e8s = ${CYCLE_FEE_E8S} : nat64;
  gas_price_buffer_bps = ${GAS_PRICE_BUFFER_BPS} : nat32;
})"
WRAP_INIT_ARGS_HEX="$(
  didc encode \
    -d "${ROOT_DIR}/othercanisters/wrap-canister/wrap_canister.did" \
    -t '(InitArgs)' \
    "${WRAP_INIT_ARGS}"
)"

log "install wrap canister"
icp canister install wrap_canister -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --mode reinstall --wasm "${WRAP_WASM}" --args-format hex --args "${WRAP_INIT_ARGS_HEX}" >/dev/null

wait_for_pattern \
  "gas price" \
  "query_pp '${ROOT_DIR}/crates/ic-evm-gateway/evm_canister.did' evm_canister rpc_eth_gas_price '()'" \
  "Ok ="

WRAP_REQUEST_ID_VEC="$(tsx_eval wrap-request-id-vec "${CALLER_PRINCIPAL}" "${LEDGER_CANISTER_ID}" "${WRAP_AMOUNT}" "0x5555555555555555555555555555555555555555" "${WRAP_EVM_NONCE}" "${WRAP_GAS_LIMIT}")"

log "wrap should fail before approve with insufficient allowance"
WRAP_NO_APPROVE_OUT="$(update_call "${ROOT_DIR}/othercanisters/wrap-canister/wrap_canister.did" wrap_canister submit_wrap_request "(record {
  request_id = ${WRAP_REQUEST_ID_VEC};
  asset_id = $(tsx_eval principal-vec "${LEDGER_CANISTER_ID}");
  amount = $(python - <<PY
value = int("${WRAP_AMOUNT}")
raw = value.to_bytes(32, "big")
print("vec { " + "; ".join(str(b) for b in raw) + " }")
PY
);
  evm_recipient = vec { $(python - <<'PY'
print("; ".join(["85"] * 20))
PY
  ) };
  evm_nonce = ${WRAP_EVM_NONCE} : nat64;
  gas_limit = ${WRAP_GAS_LIMIT} : nat64;
})" 2>&1 || true)"
[[ "${WRAP_NO_APPROVE_OUT}" == *"fee.transfer_from_failed:insufficient_allowance:"* ]] || {
  echo "${WRAP_NO_APPROVE_OUT}" >&2
  exit 1
}

log "approve ledger allowance for wrap"
update_call "${LEDGER_DID}" "${LEDGER_CANISTER_ID}" icrc2_approve "(record {
  from_subaccount = null;
  spender = record { owner = principal \"${WRAP_CANISTER_ID}\"; subaccount = null; };
  amount = ${LEDGER_APPROVE_AMOUNT} : nat;
  expected_allowance = null;
  expires_at = null;
  fee = null;
  memo = null;
  created_at_time = null;
})" >/dev/null

log "submit wrap request and wait for successful mint"
WRAP_SUBMIT_OUT="$(update_call "${ROOT_DIR}/othercanisters/wrap-canister/wrap_canister.did" wrap_canister submit_wrap_request "(record {
  request_id = ${WRAP_REQUEST_ID_VEC};
  asset_id = $(tsx_eval principal-vec "${LEDGER_CANISTER_ID}");
  amount = $(python - <<PY
value = int("${WRAP_AMOUNT}")
raw = value.to_bytes(32, "big")
print("vec { " + "; ".join(str(b) for b in raw) + " }")
PY
);
  evm_recipient = vec { $(python - <<'PY'
print("; ".join(["85"] * 20))
PY
  ) };
  evm_nonce = ${WRAP_EVM_NONCE} : nat64;
  gas_limit = ${WRAP_GAS_LIMIT} : nat64;
})")"
[[ "${WRAP_SUBMIT_OUT}" == *"variant { Ok ="* ]] || {
  echo "${WRAP_SUBMIT_OUT}" >&2
  exit 1
}

WRAP_RESULT_CMD="query_pp '${ROOT_DIR}/othercanisters/wrap-canister/wrap_canister.did' wrap_canister get_wrap_request_result '( ${WRAP_REQUEST_ID_VEC} )'"
WRAP_SUCCESS_OUT="$(wait_for_pattern "wrap success result" "${WRAP_RESULT_CMD}" "status = variant { Succeeded }")"
[[ "${WRAP_SUCCESS_OUT}" == *"fee_ledger_tx_id = opt blob"* ]] || { echo "${WRAP_SUCCESS_OUT}" >&2; exit 1; }
[[ "${WRAP_SUCCESS_OUT}" == *"pull_ledger_tx_id = opt blob"* ]] || { echo "${WRAP_SUCCESS_OUT}" >&2; exit 1; }
[[ "${WRAP_SUCCESS_OUT}" == *"mint_tx_id = opt blob"* ]] || { echo "${WRAP_SUCCESS_OUT}" >&2; exit 1; }
[[ "${WRAP_SUCCESS_OUT}" == *"mint_failed_recoverable = false"* ]] || { echo "${WRAP_SUCCESS_OUT}" >&2; exit 1; }
[[ "${WRAP_SUCCESS_OUT}" == *"error_code = null"* ]] || { echo "${WRAP_SUCCESS_OUT}" >&2; exit 1; }
WRAP_MINT_TX_ID_HEX="$(printf '%s' "${WRAP_SUCCESS_OUT}" | extract_named_opt_blob_hex "mint_tx_id")"
WRAP_MINT_RECEIPT_CMD="query_pp '${ROOT_DIR}/crates/ic-evm-gateway/evm_canister.did' evm_canister rpc_eth_get_transaction_receipt_with_status_by_tx_id '( $(hex_to_candid_vec "${WRAP_MINT_TX_ID_HEX}") )'"
WRAP_MINT_RECEIPT_OUT="$(wait_for_pattern "wrap mint receipt" "${WRAP_MINT_RECEIPT_CMD}" "status = 1 : nat8")"
[[ "${WRAP_MINT_RECEIPT_OUT}" == *"variant { Found = record"* ]] || { echo "${WRAP_MINT_RECEIPT_OUT}" >&2; exit 1; }

log "verify factory and wrapped token on evm"
FACTORY_PREDICT_CALLDATA_HEX="$(cd "${CONTRACTS_DIR}" && cast calldata "predictTokenAddress(bytes,uint8)" "0x${ASSET_ID_HEX}" "${LEDGER_DECIMALS}")"
FACTORY_GET_TOKEN_CALLDATA_HEX="$(cd "${CONTRACTS_DIR}" && cast calldata "getTokenAddress(bytes)" "0x${ASSET_ID_HEX}")"
PREDICT_OUT="$(rpc_eth_call_hex "${FACTORY_ADDRESS_HEX}" "${FACTORY_PREDICT_CALLDATA_HEX}")"
TOKEN_OUT="$(rpc_eth_call_hex "${FACTORY_ADDRESS_HEX}" "${FACTORY_GET_TOKEN_CALLDATA_HEX}")"
[[ "${PREDICT_OUT}" == *"status = 1 : nat8"* ]] || { echo "${PREDICT_OUT}" >&2; exit 1; }
[[ "${TOKEN_OUT}" == *"status = 1 : nat8"* ]] || { echo "${TOKEN_OUT}" >&2; exit 1; }
PREDICT_TOKEN_HEX="$(printf '%s' "${PREDICT_OUT}" | extract_named_blob_hex "return_data" | u256_hex_to_address_hex)"
TOKEN_ADDRESS_HEX="$(printf '%s' "${TOKEN_OUT}" | extract_named_blob_hex "return_data" | u256_hex_to_address_hex)"
[[ "${PREDICT_TOKEN_HEX}" == "${TOKEN_ADDRESS_HEX}" ]] || {
  echo "[local-wrap-unwrap-ledger] predicted token mismatch" >&2
  echo "predict=${PREDICT_TOKEN_HEX} actual=${TOKEN_ADDRESS_HEX}" >&2
  exit 1
}
TOKEN_CODE_OUT="$(query_pp "${ROOT_DIR}/crates/ic-evm-gateway/evm_canister.did" evm_canister rpc_eth_get_code "( $(hex_to_candid_vec "${TOKEN_ADDRESS_HEX}"), variant { Latest } )")"
[[ "${TOKEN_CODE_OUT}" == *"variant { Ok = blob"* ]] || { echo "${TOKEN_CODE_OUT}" >&2; exit 1; }
[[ "${TOKEN_CODE_OUT}" != *'variant { Ok = blob "" }'* ]] || { echo "${TOKEN_CODE_OUT}" >&2; exit 1; }
BALANCE_CALLDATA_HEX="$(cd "${CONTRACTS_DIR}" && cast calldata "balanceOf(address)" "0x5555555555555555555555555555555555555555")"
BALANCE_OUT="$(rpc_eth_call_hex "${TOKEN_ADDRESS_HEX}" "${BALANCE_CALLDATA_HEX}")"
[[ "${BALANCE_OUT}" == *"status = 1 : nat8"* ]] || { echo "${BALANCE_OUT}" >&2; exit 1; }
WRAP_TOKEN_BALANCE="$(printf '%s' "${BALANCE_OUT}" | extract_named_blob_hex "return_data" | u256_hex_to_decimal)"
[[ "${WRAP_TOKEN_BALANCE}" == "${WRAP_AMOUNT}" ]] || {
  echo "[local-wrap-unwrap-ledger] wrapped token balance mismatch: expected=${WRAP_AMOUNT} actual=${WRAP_TOKEN_BALANCE}" >&2
  exit 1
}
DECIMALS_CALLDATA_HEX="$(cd "${CONTRACTS_DIR}" && cast calldata "decimals()")"
DECIMALS_OUT="$(rpc_eth_call_hex "${TOKEN_ADDRESS_HEX}" "${DECIMALS_CALLDATA_HEX}")"
[[ "${DECIMALS_OUT}" == *"status = 1 : nat8"* ]] || { echo "${DECIMALS_OUT}" >&2; exit 1; }
WRAP_TOKEN_DECIMALS_ACTUAL="$(printf '%s' "${DECIMALS_OUT}" | extract_named_blob_hex "return_data" | u256_hex_to_decimal)"
[[ "${WRAP_TOKEN_DECIMALS_ACTUAL}" == "${LEDGER_DECIMALS}" ]] || {
  echo "[local-wrap-unwrap-ledger] wrapped token decimals mismatch: expected=${LEDGER_DECIMALS} actual=${WRAP_TOKEN_DECIMALS_ACTUAL}" >&2
  exit 1
}

UNWRAP_JSON="$(tsx_eval unwrap-json "${CALLER_PRINCIPAL}" "${WRAP_CANISTER_ID}" "${LEDGER_CANISTER_ID}" "${UNWRAP_AMOUNT}" "${CALLER_PRINCIPAL}" "${UNWRAP_USER_NONCE}" "${UNWRAP_DEADLINE}" "${CALLER_EVM_HEX}")"
UNWRAP_DATA_VEC="$(UNWRAP_JSON="${UNWRAP_JSON}" python - <<'PY'
import json
import os
print(json.loads(os.environ["UNWRAP_JSON"])["dataVec"])
PY
)"
UNWRAP_REQUEST_ID_VEC="$(UNWRAP_JSON="${UNWRAP_JSON}" python - <<'PY'
import json
import os
print(json.loads(os.environ["UNWRAP_JSON"])["requestIdVec"])
PY
)"
UNWRAP_PRECOMPILE_HEX="$(UNWRAP_JSON="${UNWRAP_JSON}" python - <<'PY'
import json
import os
print(json.loads(os.environ["UNWRAP_JSON"])["precompileHex"])
PY
)"

log "submit unwrap tx and wait for dispatch"
update_call "${ROOT_DIR}/crates/ic-evm-gateway/evm_canister.did" evm_canister submit_ic_tx "(record {
  to = opt vec { $(python - <<PY
hexv = "${UNWRAP_PRECOMPILE_HEX}".replace("0x", "")
print("; ".join(str(b) for b in bytes.fromhex(hexv)))
PY
) };
  value = 0 : nat;
  gas_limit = ${UNWRAP_GAS_LIMIT} : nat64;
  nonce = ${UNWRAP_USER_NONCE} : nat64;
  max_fee_per_gas = ${UNWRAP_MAX_FEE_PER_GAS} : nat;
  max_priority_fee_per_gas = 300000000000 : nat;
  data = ${UNWRAP_DATA_VEC};
})" >/dev/null

DISPATCH_CMD="query_pp '${ROOT_DIR}/crates/ic-evm-gateway/evm_canister.did' evm_canister get_request_dispatch_result '(variant { Unwrap }, ${UNWRAP_REQUEST_ID_VEC})'"
DISPATCH_OUT="$(wait_for_pattern "unwrap dispatch" "${DISPATCH_CMD}" "status = variant { Dispatched }")"
[[ "${DISPATCH_OUT}" == *"error_code = null"* ]] || { echo "${DISPATCH_OUT}" >&2; exit 1; }

WRAP_UNWRAP_CMD="query_pp '${ROOT_DIR}/othercanisters/wrap-canister/wrap_canister.did' wrap_canister get_request_result '( ${UNWRAP_REQUEST_ID_VEC} )'"
UNWRAP_RESULT_OUT="$(wait_for_pattern "unwrap execution" "${WRAP_UNWRAP_CMD}" "status = variant { Succeeded }")"
[[ "${UNWRAP_RESULT_OUT}" == *"ledger_tx_id = opt blob"* ]] || { echo "${UNWRAP_RESULT_OUT}" >&2; exit 1; }

log "smoke completed"
log "ledger=${LEDGER_CANISTER_ID} evm=${EVM_CANISTER_ID} wrap=${WRAP_CANISTER_ID}"
