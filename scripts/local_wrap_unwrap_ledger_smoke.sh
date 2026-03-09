#!/usr/bin/env bash
# where: local managed network smoke
# what: official ICRC ledger を deploy して wrap/unwrap を end-to-end で確認する
# why: dummy ledger では見えない fee pull / withdraw / unwrap transfer を local で再現するため
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
NETWORK="${NETWORK:-local}"
ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME:-}"
LEDGER_CACHE_DIR="${LEDGER_CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/kasane-local-ledger}"
LEDGER_RELEASE="${LEDGER_RELEASE:-latest}"
LEDGER_WASM_GZ="${LEDGER_CACHE_DIR}/ic-icrc1-ledger.wasm.gz"
LEDGER_DID="${LEDGER_CACHE_DIR}/ledger.did"
LEDGER_WASM="${LEDGER_CACHE_DIR}/ic-icrc1-ledger.wasm"
WRAP_FACTORY_HEX="${WRAP_FACTORY_HEX:-1111111111111111111111111111111111111111}"
WRAP_AMOUNT="${WRAP_AMOUNT:-1000000}"
WRAP_GAS_LIMIT="${WRAP_GAS_LIMIT:-150000}"
WRAP_BAD_EVM_NONCE="${WRAP_BAD_EVM_NONCE:-1}"
UNWRAP_AMOUNT="${UNWRAP_AMOUNT:-1000000}"
UNWRAP_GAS_LIMIT="${UNWRAP_GAS_LIMIT:-300000}"
UNWRAP_USER_NONCE="${UNWRAP_USER_NONCE:-0}"
UNWRAP_DEADLINE="${UNWRAP_DEADLINE:-18446744073709551615}"
LEDGER_INITIAL_USER_BALANCE="${LEDGER_INITIAL_USER_BALANCE:-5000000000}"
LEDGER_INITIAL_WRAP_BALANCE="${LEDGER_INITIAL_WRAP_BALANCE:-5000000000}"
LEDGER_APPROVE_AMOUNT="${LEDGER_APPROVE_AMOUNT:-5000000000}"
LEDGER_TRANSFER_FEE="${LEDGER_TRANSFER_FEE:-10}"
CYCLE_FEE_E8S="${CYCLE_FEE_E8S:-1000000}"
GAS_PRICE_BUFFER_BPS="${GAS_PRICE_BUFFER_BPS:-12000}"
GENESIS_BALANCE_WEI="${GENESIS_BALANCE_WEI:-1000000000000000000}"
SEED_TX_TAG="${SEED_TX_TAG:-$(python - <<'PY'
import time
print(int(time.time_ns()) & 0xff)
PY
)}"
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
require_cmd curl
require_cmd dfx
require_cmd didc
require_cmd gzip
require_cmd icp
require_cmd node
require_cmd npm
require_cmd python

if [[ "${#WRAP_FACTORY_HEX}" -ne 40 ]]; then
  echo "[local-wrap-unwrap-ledger] WRAP_FACTORY_HEX must be 40 hex chars" >&2
  exit 1
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

log "create canisters"
LEDGER_CANISTER_ID="$(retry_command_output "create detached ledger canister" icp canister create -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --detached -q)"
retry_command_output "create evm_canister" icp canister create -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" evm_canister >/dev/null
retry_command_output "create wrap_canister" icp canister create -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" wrap_canister >/dev/null
EVM_CANISTER_ID="$(retry_command_output "resolve evm canister id" icp canister status -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --id-only evm_canister)"
WRAP_CANISTER_ID="$(retry_command_output "resolve wrap canister id" icp canister status -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --id-only wrap_canister)"
CALLER_PRINCIPAL="$(icp identity principal --identity "${ICP_IDENTITY_NAME}")"
LEDGER_MINTER_PRINCIPAL="${LEDGER_MINTER_PRINCIPAL:-${EVM_CANISTER_ID}}"
CALLER_EVM_HEX="$(cargo run -q -p ic-evm-core --bin derive_evm_address -- "${CALLER_PRINCIPAL}")"
LEDGER_INIT_ARGS="(variant { Init = record {
  token_symbol = \"LICP\";
  token_name = \"Local ICP\";
  minting_account = record { owner = principal \"${LEDGER_MINTER_PRINCIPAL}\"; subaccount = null; };
  fee_collector_account = null;
  transfer_fee = ${LEDGER_TRANSFER_FEE};
  decimals = null;
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
  genesis_balances = vec { record { address = vec { $(python - <<PY
hexv = "${CALLER_EVM_HEX}".strip()
print("; ".join(str(b) for b in bytes.fromhex(hexv)))
PY
) }; amount = ${GENESIS_BALANCE_WEI} : nat; } };
  wrap_canister_id = principal \"${WRAP_CANISTER_ID}\";
})"
GATEWAY_INIT_ARGS_HEX="$(
  didc encode \
    -d "${ROOT_DIR}/crates/ic-evm-gateway/evm_canister.did" \
    -t '(opt InitArgs)' \
    "${GATEWAY_INIT_ARGS}"
)"
WRAP_INIT_ARGS="(record {
  kasane_canister = principal \"${EVM_CANISTER_ID}\";
  evm_gateway_canister = principal \"${EVM_CANISTER_ID}\";
  evm_wrap_factory = vec { $(python - <<PY
hexv = "${WRAP_FACTORY_HEX}"
print("; ".join(str(b) for b in bytes.fromhex(hexv)))
PY
) };
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

log "install ledger"
icp canister install "${LEDGER_CANISTER_ID}" -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --mode install --wasm "${LEDGER_WASM}" --args-format hex --args "${LEDGER_INIT_ARGS_HEX}" >/dev/null

log "install gateway"
icp canister install evm_canister -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --mode reinstall --wasm "${GATEWAY_WASM}" --args-format hex --args "${GATEWAY_INIT_ARGS_HEX}" >/dev/null

log "install wrap canister"
icp canister install wrap_canister -e "${NETWORK}" --identity "${ICP_IDENTITY_NAME}" --mode reinstall --wasm "${WRAP_WASM}" --args-format hex --args "${WRAP_INIT_ARGS_HEX}" >/dev/null

log "seed gas price by submitting one normal tx"
update_call "${ROOT_DIR}/crates/ic-evm-gateway/evm_canister.did" evm_canister submit_ic_tx "(record {
  to = opt vec { $(python - <<'PY'
print("; ".join(["16"] * 20))
PY
) };
  value = 0 : nat;
  gas_limit = 50000 : nat64;
  nonce = 0 : nat64;
  max_fee_per_gas = 600000000000 : nat;
  max_priority_fee_per_gas = 300000000000 : nat;
  data = vec { ${SEED_TX_TAG} };
})" >/dev/null

wait_for_pattern \
  "gas price" \
  "query_pp '${ROOT_DIR}/crates/ic-evm-gateway/evm_canister.did' evm_canister rpc_eth_gas_price '()'" \
  "Ok ="

WRAP_REQUEST_ID_VEC="$(tsx_eval wrap-request-id-vec "${CALLER_PRINCIPAL}" "${LEDGER_CANISTER_ID}" "${WRAP_AMOUNT}" "0x5555555555555555555555555555555555555555" "${WRAP_BAD_EVM_NONCE}" "${WRAP_GAS_LIMIT}")"

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
  evm_nonce = ${WRAP_BAD_EVM_NONCE} : nat64;
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

log "submit wrap request and wait for recoverable mint failure"
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
  evm_nonce = ${WRAP_BAD_EVM_NONCE} : nat64;
  gas_limit = ${WRAP_GAS_LIMIT} : nat64;
})")"
[[ "${WRAP_SUBMIT_OUT}" == *"variant { Ok ="* ]] || {
  echo "${WRAP_SUBMIT_OUT}" >&2
  exit 1
}

WRAP_RESULT_CMD="query_pp '${ROOT_DIR}/othercanisters/wrap-canister/wrap_canister.did' wrap_canister get_wrap_request_result '( ${WRAP_REQUEST_ID_VEC} )'"
WRAP_FAILED_OUT="$(wait_for_pattern "wrap failed result" "${WRAP_RESULT_CMD}" "mint_failed_recoverable = true")"
[[ "${WRAP_FAILED_OUT}" == *"fee_ledger_tx_id = opt blob"* ]] || { echo "${WRAP_FAILED_OUT}" >&2; exit 1; }
[[ "${WRAP_FAILED_OUT}" == *"pull_ledger_tx_id = opt blob"* ]] || { echo "${WRAP_FAILED_OUT}" >&2; exit 1; }

log "withdraw failed wrap"
WITHDRAW_OUT="$(update_call "${ROOT_DIR}/othercanisters/wrap-canister/wrap_canister.did" wrap_canister withdraw_failed_wrap "(record { request_id = ${WRAP_REQUEST_ID_VEC}; })")"
[[ "${WITHDRAW_OUT}" == *"variant { Ok ="* ]] || {
  echo "${WITHDRAW_OUT}" >&2
  exit 1
}
WRAP_WITHDRAWN_OUT="$(wait_for_pattern "wrap withdrawn result" "${WRAP_RESULT_CMD}" "withdrawn = true")"
[[ "${WRAP_WITHDRAWN_OUT}" == *"withdraw_ledger_tx_id = opt blob"* ]] || { echo "${WRAP_WITHDRAWN_OUT}" >&2; exit 1; }

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
  nonce = 1 : nat64;
  max_fee_per_gas = 600000000000 : nat;
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
