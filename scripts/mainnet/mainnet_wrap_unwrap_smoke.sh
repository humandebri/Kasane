#!/usr/bin/env bash
# where: mainnet wrap/unwrap smoke
# what: TESTICP を使って wrap -> unwrap の実経路を最小手順で検証する
# why: 手打ちの request_id / payload / gas_limit ミスを避けつつ証跡を残すため
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

source "${REPO_ROOT}/scripts/lib_candid_result.sh"

ICP_ENV="${ICP_ENV:-ic}"
ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME:-ci-local}"
EVM_CANISTER_ID="${EVM_CANISTER_ID:-4c52m-aiaaa-aaaam-agwwa-cai}"
WRAP_CANISTER_ID="${WRAP_CANISTER_ID:-lpuz5-uyaaa-aaaam-ah4da-cai}"
FEE_LEDGER_CANISTER_ID="${FEE_LEDGER_CANISTER_ID:-xafvr-biaaa-aaaai-aql5q-cai}"
FEE_LEDGER_DECIMALS="${FEE_LEDGER_DECIMALS:-8}"
EVM_WRAP_FACTORY="${EVM_WRAP_FACTORY:-}"
WRAP_AMOUNT_E8S="${WRAP_AMOUNT_E8S:-1000000}"
WRAP_ALLOWANCE_E8S="${WRAP_ALLOWANCE_E8S:-500000000}"
UNWRAP_AMOUNT_E8S="${UNWRAP_AMOUNT_E8S:-${WRAP_AMOUNT_E8S}}"
UNWRAP_RECIPIENT_PRINCIPAL="${UNWRAP_RECIPIENT_PRINCIPAL:-}"
REPORT_DIR="${REPORT_DIR:-docs/ops/reports}"
WAIT_RETRIES="${WAIT_RETRIES:-30}"
WAIT_SECONDS="${WAIT_SECONDS:-2}"
HELPER_TS="$(mktemp "${REPO_ROOT}/tools/wrapper/.mainnet-wrap-unwrap-helper.XXXXXX.mts")"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
REPORT_FILE="${REPORT_DIR}/mainnet-wrap-unwrap-smoke-${TIMESTAMP}.md"
REQUEST_STATUS_HASH="100_394_802"
REQUEST_STATUS_SUCCEEDED_HASH="2_633_774_657"
WRAP_RESULT_CHARGED_FEE_HASH="435_439_640"
WRAP_RESULT_MINT_TX_ID_HASH="3_860_632_153"

cleanup() {
  rm -f "${HELPER_TS}"
}
trap cleanup EXIT

log() {
  echo "[mainnet-wrap-unwrap] $*"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "[mainnet-wrap-unwrap] missing command: $1" >&2
    exit 1
  }
}

require_cmd icp
require_cmd node
require_cmd python

if [[ -z "${EVM_WRAP_FACTORY}" ]]; then
  echo "[mainnet-wrap-unwrap] EVM_WRAP_FACTORY is required" >&2
  exit 1
fi

mkdir -p "${REPORT_DIR}"

CALLER_PRINCIPAL="$(icp identity principal --identity "${ICP_IDENTITY_NAME}")"
if [[ -z "${UNWRAP_RECIPIENT_PRINCIPAL}" ]]; then
  UNWRAP_RECIPIENT_PRINCIPAL="${CALLER_PRINCIPAL}"
fi

cat > "${HELPER_TS}" <<'TS'
import { Principal } from "@dfinity/principal";
// HELPER_TS is created under tools/wrapper, so ./lib/* stays portable across hosts.
import { callerEvmAddressFromPrincipalText } from "./lib/principal.ts";
import {
  decimalToBytes32,
  deriveWrapRequestId,
  toSubmitIcTxData,
} from "./lib/request-id.ts";
import {
  encodeApproveCall,
  encodeFactoryGetTokenAddressCall,
} from "./lib/erc20.ts";
import {
  applyUnwrapGasHeadroom,
  buildUnwrapEstimateCallObject,
  buildWrapEstimateCallObject,
} from "./lib/wrap-estimate.ts";
import { hexToBytes } from "./lib/utils.ts";

function toVec(bytes: Uint8Array): string {
  return `vec { ${Array.from(bytes).join("; ")} }`;
}

const mode = process.argv[2] ?? "";
if (mode === "wrap-meta") {
  const principal = process.argv[3] ?? "";
  const assetId = process.argv[4] ?? "";
  const amount = process.argv[5] ?? "";
  const factory = process.argv[6] ?? "";
  const evmNonce = BigInt(process.argv[7] ?? "0");
  const gasLimit = BigInt(process.argv[8] ?? "0");
  const tokenDecimals = Number(process.argv[10] ?? "0");
  const callerPrincipal = Principal.fromText(principal);
  const assetPrincipal = Principal.fromText(assetId);
  const evmRecipient = callerEvmAddressFromPrincipalText(principal);
  const estimateCall = buildWrapEstimateCallObject({
    wrapCanisterId: process.argv[9] ?? "",
    evmWrapFactory: factory,
    assetId,
    tokenDecimals,
    amount,
    evmRecipient: `0x${Buffer.from(evmRecipient).toString("hex")}`,
  });
  const requestId = deriveWrapRequestId({
    fromOwner: callerPrincipal.toUint8Array(),
    assetId: assetPrincipal.toUint8Array(),
    amount: decimalToBytes32(amount),
    evmRecipient,
    evmNonce,
    gasLimit,
  });
  process.stdout.write(JSON.stringify({
    caller_evm_hex: Buffer.from(evmRecipient).toString("hex"),
    wrap_evm_hex: Buffer.from(callerEvmAddressFromPrincipalText(process.argv[9] ?? "")).toString("hex"),
    asset_hex: Buffer.from(assetPrincipal.toUint8Array()).toString("hex"),
    amount_hex: Buffer.from(decimalToBytes32(amount)).toString("hex"),
    evm_recipient_hex: Buffer.from(evmRecipient).toString("hex"),
    evm_nonce: evmNonce.toString(),
    request_id_hex: Buffer.from(requestId).toString("hex"),
    asset_vec: toVec(assetPrincipal.toUint8Array()),
    amount_vec: toVec(decimalToBytes32(amount)),
    evm_recipient_vec: toVec(evmRecipient),
    request_id_vec: toVec(requestId),
    estimate_to_vec: toVec(estimateCall.to[0] ?? new Uint8Array()),
    estimate_from_vec: toVec(estimateCall.from[0] ?? new Uint8Array()),
    estimate_value_vec: toVec(estimateCall.value[0] ?? new Uint8Array()),
    estimate_data_vec: toVec(estimateCall.data[0] ?? new Uint8Array()),
  }));
} else if (mode === "unwrap-meta") {
  const principal = process.argv[3] ?? "";
  const assetId = process.argv[4] ?? "";
  const amount = BigInt(process.argv[5] ?? "0");
  const nonce = BigInt(process.argv[6] ?? "0");
  const recipient = process.argv[7] ?? "";
  const callerEvm = callerEvmAddressFromPrincipalText(principal);
  const data = toSubmitIcTxData({ assetId, amount, recipient });
  const estimateCall = buildUnwrapEstimateCallObject({
    callerEvmAddress: callerEvm,
    nonce,
    data,
  });
  process.stdout.write(JSON.stringify({
    caller_evm_hex: Buffer.from(callerEvm).toString("hex"),
    data_vec: toVec(data),
    estimate_to_vec: toVec(estimateCall.to[0] ?? new Uint8Array()),
    estimate_from_vec: toVec(estimateCall.from[0] ?? new Uint8Array()),
    estimate_value_vec: toVec(estimateCall.value[0] ?? new Uint8Array()),
    estimate_data_vec: toVec(estimateCall.data[0] ?? new Uint8Array()),
    unwrap_headroom: applyUnwrapGasHeadroom(50074n).toString(),
  }));
} else if (mode === "factory-get-token-data") {
  const assetId = process.argv[3] ?? "";
  process.stdout.write(Buffer.from(encodeFactoryGetTokenAddressCall(assetId)).toString("hex"));
} else if (mode === "approve-data") {
  const spenderHex = process.argv[3] ?? "";
  const amount = BigInt(process.argv[4] ?? "0");
  process.stdout.write(Buffer.from(encodeApproveCall(hexToBytes(spenderHex), amount)).toString("hex"));
} else if (mode === "approve-estimate") {
  const ownerHex = process.argv[3] ?? "";
  const tokenHex = process.argv[4] ?? "";
  const spenderHex = process.argv[5] ?? "";
  const amount = BigInt(process.argv[6] ?? "0");
  process.stdout.write(JSON.stringify({
    from_vec: toVec(hexToBytes(ownerHex)),
    to_vec: toVec(hexToBytes(tokenHex)),
    value_vec: toVec(new Uint8Array(32)),
    data_vec: toVec(encodeApproveCall(hexToBytes(spenderHex), amount)),
  }));
} else {
  throw new Error(`unknown mode: ${mode}`);
}
TS

helper_json() {
  (
    cd "${REPO_ROOT}/tools/wrapper"
    node --import tsx/esm "${HELPER_TS}" "$@"
  )
}

helper_text() {
  (
    cd "${REPO_ROOT}/tools/wrapper"
    node --import tsx/esm "${HELPER_TS}" "$@"
  )
}

extract_nat() {
  OUTPUT_TEXT="$1" python - <<'PY'
import os, re
text = os.environ["OUTPUT_TEXT"]
m = re.search(r'Ok\s*=\s*([0-9_]+)\s*:\s*(?:nat|nat64)', text)
if not m:
    raise SystemExit(1)
print(m.group(1).replace('_', ''))
PY
}

extract_named_blob_hex() {
  local text
  local label
  if [[ $# -eq 1 ]]; then
    text="$(cat)"
    label="$1"
  else
    text="$1"
    label="$2"
  fi
  OUTPUT_TEXT="${text}" LABEL_TEXT="${label}" python - <<'PY'
import os, re
text = os.environ["OUTPUT_TEXT"]
label = re.escape(os.environ["LABEL_TEXT"])
m = re.search(label + r'\s*=\s*(?:opt\s+)?blob\s+"((?:[^"\\]|\\.)*)"', text)
if not m:
    raise SystemExit(1)
s = m.group(1)
out = bytearray()
i = 0
while i < len(s):
    if s[i] == '\\' and i + 2 < len(s) and all(c in '0123456789abcdefABCDEF' for c in s[i + 1:i + 3]):
        out.append(int(s[i + 1:i + 3], 16))
        i += 3
    elif s[i] == '\\' and i + 1 < len(s):
        out.append(ord(s[i + 1]))
        i += 2
    else:
        out.append(ord(s[i]))
        i += 1
print(out.hex())
PY
}

extract_first_blob_hex() {
  OUTPUT_TEXT="$1" python - <<'PY'
import os, re
text = os.environ["OUTPUT_TEXT"]
m = re.search(r'blob\s+"((?:[^"\\]|\\.)*)"', text)
if not m:
    raise SystemExit(1)
s = m.group(1)
out = bytearray()
i = 0
while i < len(s):
    if s[i] == '\\' and i + 2 < len(s) and all(c in '0123456789abcdefABCDEF' for c in s[i + 1:i + 3]):
        out.append(int(s[i + 1:i + 3], 16))
        i += 3
    elif s[i] == '\\' and i + 1 < len(s):
        out.append(ord(s[i + 1]))
        i += 2
    else:
        out.append(ord(s[i]))
        i += 1
print(out.hex())
PY
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

hex_to_candid_blob() {
  python - "$1" <<'PY'
import sys
hexv = sys.argv[1].strip().lower()
if hexv.startswith("0x"):
    hexv = hexv[2:]
print('blob "' + ''.join(f'\\{hexv[i:i+2]}' for i in range(0, len(hexv), 2)) + '"')
PY
}

u256_hex_to_address_hex() {
  python - <<'PY'
import sys
hexv = sys.stdin.read().strip().lower()
if hexv.startswith("0x"):
    hexv = hexv[2:]
print(hexv[-40:])
PY
}

rpc_eth_call_hex() {
  local to_hex="$1"
  local data_hex="$2"
  icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${EVM_CANISTER_ID}" rpc_eth_call_object "(record { to = opt $(hex_to_candid_vec "${to_hex}"); gas = opt 500000 : opt nat64; value = null; data = opt $(hex_to_candid_vec "${data_hex}"); from = opt $(hex_to_candid_vec "${CALLER_EVM_HEX}"); max_fee_per_gas = null; max_priority_fee_per_gas = null; chain_id = null; nonce = null; tx_type = null; access_list = null; gas_price = null })"
}

wait_until() {
  local label="$1"
  local pattern="$2"
  shift 2
  local out=""
  local i
  for i in $(seq 1 "${WAIT_RETRIES}"); do
    out="$("$@" 2>&1)"
    if [[ "${out}" == *"${pattern}"* ]]; then
      printf '%s\n' "${out}"
      return 0
    fi
    sleep "${WAIT_SECONDS}"
  done
  echo "[mainnet-wrap-unwrap] ${label} timeout: ${pattern}" >&2
  printf '%s\n' "${out}" >&2
  return 1
}

append_report() {
  printf '%s\n' "$1" >> "${REPORT_FILE}"
}

BALANCE_OUT="$(icp token "${FEE_LEDGER_CANISTER_ID}" balance -n ic --identity "${ICP_IDENTITY_NAME}")"
ALLOWANCE_OUT="$(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" "${FEE_LEDGER_CANISTER_ID}" icrc2_allowance "(record { account = record { owner = principal \"${CALLER_PRINCIPAL}\"; subaccount = null }; spender = record { owner = principal \"${WRAP_CANISTER_ID}\"; subaccount = null } })")"
WRAP_ESTIMATE_META="$(helper_json wrap-meta "${CALLER_PRINCIPAL}" "${FEE_LEDGER_CANISTER_ID}" "${WRAP_AMOUNT_E8S}" "${EVM_WRAP_FACTORY}" "0" "1" "${WRAP_CANISTER_ID}" "${FEE_LEDGER_DECIMALS}")"
CALLER_EVM_HEX="$(WRAP_ESTIMATE_META="${WRAP_ESTIMATE_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["WRAP_ESTIMATE_META"])["caller_evm_hex"])
PY
)"
WRAP_EVM_HEX="$(WRAP_ESTIMATE_META="${WRAP_ESTIMATE_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["WRAP_ESTIMATE_META"])["wrap_evm_hex"])
PY
)"
WRAP_NONCE="$(extract_nat "$(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${EVM_CANISTER_ID}" expected_nonce_by_address "(vec { $(python - <<PY
hexv = "${WRAP_EVM_HEX}"
print("; ".join(str(b) for b in bytes.fromhex(hexv)))
PY
) })")")"

WRAP_ESTIMATE_META="$(helper_json wrap-meta "${CALLER_PRINCIPAL}" "${FEE_LEDGER_CANISTER_ID}" "${WRAP_AMOUNT_E8S}" "${EVM_WRAP_FACTORY}" "${WRAP_NONCE}" "1" "${WRAP_CANISTER_ID}" "${FEE_LEDGER_DECIMALS}")"
WRAP_ESTIMATE_OUT="$(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${EVM_CANISTER_ID}" rpc_eth_estimate_gas_object "(record { to = opt $(WRAP_ESTIMATE_META="${WRAP_ESTIMATE_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["WRAP_ESTIMATE_META"])["estimate_to_vec"])
PY
); gas = null; value = opt $(WRAP_ESTIMATE_META="${WRAP_ESTIMATE_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["WRAP_ESTIMATE_META"])["estimate_value_vec"])
PY
); max_priority_fee_per_gas = null; data = opt $(WRAP_ESTIMATE_META="${WRAP_ESTIMATE_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["WRAP_ESTIMATE_META"])["estimate_data_vec"])
PY
); from = opt $(WRAP_ESTIMATE_META="${WRAP_ESTIMATE_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["WRAP_ESTIMATE_META"])["estimate_from_vec"])
PY
); max_fee_per_gas = null; chain_id = null; nonce = null; tx_type = null; access_list = null; gas_price = null })")"
WRAP_GAS_LIMIT="$(
  WRAP_ESTIMATE="$(extract_nat "${WRAP_ESTIMATE_OUT}")" python - <<'PY'
import os
value = int(os.environ["WRAP_ESTIMATE"])
print((value * 12 + 9) // 10)
PY
)"

WRAP_META="$(helper_json wrap-meta "${CALLER_PRINCIPAL}" "${FEE_LEDGER_CANISTER_ID}" "${WRAP_AMOUNT_E8S}" "${EVM_WRAP_FACTORY}" "${WRAP_NONCE}" "${WRAP_GAS_LIMIT}" "${WRAP_CANISTER_ID}" "${FEE_LEDGER_DECIMALS}")"
WRAP_RECIPIENT_BLOB="$(WRAP_META="${WRAP_META}" python - <<'PY'
import json, os
hexv = json.loads(os.environ["WRAP_META"])["evm_recipient_hex"]
print(''.join(f'\\{hexv[i:i+2]}' for i in range(0, len(hexv), 2)))
PY
)"
WRAP_QUOTE_OUT="$(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${WRAP_CANISTER_ID}" quote_wrap_request "(record { asset_id = principal \"${FEE_LEDGER_CANISTER_ID}\"; amount_e8s = ${WRAP_AMOUNT_E8S} : nat; evm_recipient = blob \"${WRAP_RECIPIENT_BLOB}\"; gas_limit = ${WRAP_GAS_LIMIT} : nat64 })")"
candid_is_ok "${WRAP_QUOTE_OUT}" >/dev/null
WRAP_QUOTED_FEE_E8S="$(OUTPUT_TEXT="${WRAP_QUOTE_OUT}" python - <<'PY'
import os, re
m = re.search(r'charged_fee_e8s\s*=\s*([0-9_]+)\s*:\s*nat', os.environ["OUTPUT_TEXT"])
print(m.group(1).replace('_', '') if m else '')
PY
)"

log "approve fee ledger if needed"
icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" "${FEE_LEDGER_CANISTER_ID}" icrc2_approve "(record { from_subaccount = null; spender = record { owner = principal \"${WRAP_CANISTER_ID}\"; subaccount = null }; amount = ${WRAP_ALLOWANCE_E8S} : nat; expected_allowance = null; expires_at = null; fee = null; memo = null; created_at_time = null })" >/dev/null

log "submit wrap request"
WRAP_SUBMIT_OUT="$(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" "${WRAP_CANISTER_ID}" submit_wrap_request "(record { asset_id = principal \"${FEE_LEDGER_CANISTER_ID}\"; amount_e8s = ${WRAP_AMOUNT_E8S} : nat; evm_recipient = blob \"${WRAP_RECIPIENT_BLOB}\"; gas_limit = ${WRAP_GAS_LIMIT} : nat64 })")"
candid_is_ok "${WRAP_SUBMIT_OUT}" >/dev/null

WRAP_REQUEST_ID_HEX="$(extract_first_blob_hex "${WRAP_SUBMIT_OUT}")"
WRAP_RESULT_OUT="$(wait_until "wrap_result" "${REQUEST_STATUS_HASH} = variant { ${REQUEST_STATUS_SUCCEEDED_HASH} }" icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${WRAP_CANISTER_ID}" get_request "(blob \"$(python - <<PY
hexv = "${WRAP_REQUEST_ID_HEX}"
print(''.join(f'\\\\{hexv[i:i+2]}' for i in range(0, len(hexv), 2)))
PY
)\")")"
MINT_TX_ID_HEX="$(extract_named_blob_hex "${WRAP_RESULT_OUT}" "${WRAP_RESULT_MINT_TX_ID_HASH}")"
WRAP_FEE_E8S="$(OUTPUT_TEXT="${WRAP_RESULT_OUT}" python - <<'PY'
import os, re
m = re.search(r'435_439_640 = opt \(([0-9_]+) : nat\)', os.environ["OUTPUT_TEXT"])
print(m.group(1).replace('_', '') if m else '')
PY
)"
MINT_RECEIPT_OUT="$(wait_until "mint_receipt" "status = 1 : nat8" icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${EVM_CANISTER_ID}" rpc_eth_get_transaction_receipt_with_status_by_tx_id "(blob \"$(python - <<PY
hexv = "${MINT_TX_ID_HEX}"
print(''.join(f'\\\\{hexv[i:i+2]}' for i in range(0, len(hexv), 2)))
PY
)\")")"

GAS_PRICE="$(extract_nat "$(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${EVM_CANISTER_ID}" rpc_eth_gas_price '()')")"
PRIORITY_FEE="$(extract_nat "$(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${EVM_CANISTER_ID}" rpc_eth_max_priority_fee_per_gas '()')")"
UNWRAP_REQS_OUT="$(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${WRAP_CANISTER_ID}" get_unwrap_requirements "(record { asset_id = principal \"${FEE_LEDGER_CANISTER_ID}\"; amount_e8s = ${UNWRAP_AMOUNT_E8S} : nat; caller_evm_address = $(hex_to_candid_blob "${CALLER_EVM_HEX}") })")"
WRAPPED_TOKEN_HEX="$(printf '%s' "${UNWRAP_REQS_OUT}" | extract_named_blob_hex "wrapped_token_address")"
APPROVE_ESTIMATE_META="$(helper_json approve-estimate "${CALLER_EVM_HEX}" "${WRAPPED_TOKEN_HEX}" "${EVM_WRAP_FACTORY}" "${UNWRAP_AMOUNT_E8S}")"
APPROVE_ESTIMATE_OUT="$(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${EVM_CANISTER_ID}" rpc_eth_estimate_gas_object "(record { to = opt $(APPROVE_ESTIMATE_META="${APPROVE_ESTIMATE_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["APPROVE_ESTIMATE_META"])["to_vec"])
PY
); gas = null; value = opt $(APPROVE_ESTIMATE_META="${APPROVE_ESTIMATE_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["APPROVE_ESTIMATE_META"])["value_vec"])
PY
); max_priority_fee_per_gas = null; data = opt $(APPROVE_ESTIMATE_META="${APPROVE_ESTIMATE_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["APPROVE_ESTIMATE_META"])["data_vec"])
PY
); from = opt $(APPROVE_ESTIMATE_META="${APPROVE_ESTIMATE_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["APPROVE_ESTIMATE_META"])["from_vec"])
PY
); max_fee_per_gas = null; chain_id = null; nonce = null; tx_type = null; access_list = null; gas_price = null })")"
APPROVE_ESTIMATE_GAS="$(extract_nat "${APPROVE_ESTIMATE_OUT}")"
APPROVE_GAS_LIMIT="$(
  APPROVE_ESTIMATE_GAS="${APPROVE_ESTIMATE_GAS}" python - <<'PY'
import os
value = int(os.environ["APPROVE_ESTIMATE_GAS"])
print((value * 12 + 9) // 10)
PY
)"
APPROVE_CALLDATA_HEX="$(helper_text approve-data "${EVM_WRAP_FACTORY}" "${UNWRAP_AMOUNT_E8S}")"
APPROVE_NONCE="$(extract_nat "$(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${EVM_CANISTER_ID}" expected_nonce_by_address "(vec { $(python - <<PY
hexv = "${CALLER_EVM_HEX}"
print('; '.join(str(b) for b in bytes.fromhex(hexv)))
PY
) })")")"
log "approve wrapped token for unwrap burn"
APPROVE_SUBMIT_OUT="$(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" "${EVM_CANISTER_ID}" submit_ic_tx "(record { to = opt $(hex_to_candid_vec "${WRAPPED_TOKEN_HEX}"); value = 0 : nat; gas_limit = ${APPROVE_GAS_LIMIT} : nat64; nonce = ${APPROVE_NONCE} : nat64; max_fee_per_gas = ${GAS_PRICE} : nat; max_priority_fee_per_gas = ${PRIORITY_FEE} : nat; data = $(hex_to_candid_vec "${APPROVE_CALLDATA_HEX}") })")"
candid_is_ok "${APPROVE_SUBMIT_OUT}" >/dev/null
APPROVE_TX_ID_HEX="$(extract_named_blob_hex "${APPROVE_SUBMIT_OUT}" "Ok")"
APPROVE_RECEIPT_OUT="$(wait_until "approve_receipt" "status = 1 : nat8" icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${EVM_CANISTER_ID}" rpc_eth_get_transaction_receipt_with_status_by_tx_id "(blob \"$(python - <<PY
hexv = "${APPROVE_TX_ID_HEX}"
print(''.join(f'\\\\{hexv[i:i+2]}' for i in range(0, len(hexv), 2)))
PY
)\")")"

UNWRAP_NONCE="$(extract_nat "$(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${EVM_CANISTER_ID}" expected_nonce_by_address "(vec { $(python - <<PY
hexv = "${CALLER_EVM_HEX}"
print('; '.join(str(b) for b in bytes.fromhex(hexv)))
PY
) })")")"
UNWRAP_META="$(helper_json unwrap-meta "${CALLER_PRINCIPAL}" "${FEE_LEDGER_CANISTER_ID}" "${UNWRAP_AMOUNT_E8S}" "${UNWRAP_NONCE}" "${UNWRAP_RECIPIENT_PRINCIPAL}")"
UNWRAP_ESTIMATE_OUT="$(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${EVM_CANISTER_ID}" rpc_eth_estimate_gas_object "(record { to = opt $(UNWRAP_META="${UNWRAP_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["UNWRAP_META"])["estimate_to_vec"])
PY
); gas = null; value = opt $(UNWRAP_META="${UNWRAP_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["UNWRAP_META"])["estimate_value_vec"])
PY
); max_priority_fee_per_gas = null; data = opt $(UNWRAP_META="${UNWRAP_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["UNWRAP_META"])["estimate_data_vec"])
PY
); from = opt $(UNWRAP_META="${UNWRAP_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["UNWRAP_META"])["estimate_from_vec"])
PY
); max_fee_per_gas = null; chain_id = null; nonce = opt (${UNWRAP_NONCE} : nat64); tx_type = null; access_list = null; gas_price = null })")"
UNWRAP_ESTIMATE_GAS="$(extract_nat "${UNWRAP_ESTIMATE_OUT}")"
UNWRAP_GAS_LIMIT="$(
  UNWRAP_ESTIMATE_GAS="${UNWRAP_ESTIMATE_GAS}" python - <<'PY'
import os
value = (int(os.environ["UNWRAP_ESTIMATE_GAS"]) * 12 + 9) // 10
print(max(value, 300000))
PY
)"

log "submit unwrap tx"
UNWRAP_SUBMIT_OUT="$(icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" "${EVM_CANISTER_ID}" submit_ic_tx "(record { to = opt vec { 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 0; 255; 255; 0; 1 }; value = 0 : nat; gas_limit = ${UNWRAP_GAS_LIMIT} : nat64; nonce = ${UNWRAP_NONCE} : nat64; max_fee_per_gas = ${GAS_PRICE} : nat; max_priority_fee_per_gas = ${PRIORITY_FEE} : nat; data = $(UNWRAP_META="${UNWRAP_META}" python - <<'PY'
import json, os
print(json.loads(os.environ["UNWRAP_META"])["data_vec"])
PY
) })")"
candid_is_ok "${UNWRAP_SUBMIT_OUT}" >/dev/null
UNWRAP_TX_ID_HEX="$(extract_named_blob_hex "${UNWRAP_SUBMIT_OUT}" "Ok")"

UNWRAP_RECEIPT_OUT="$(wait_until "unwrap_receipt" "status = 1 : nat8" icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${EVM_CANISTER_ID}" rpc_eth_get_transaction_receipt_with_status_by_tx_id "(blob \"$(python - <<PY
hexv = "${UNWRAP_TX_ID_HEX}"
print(''.join(f'\\\\{hexv[i:i+2]}' for i in range(0, len(hexv), 2)))
PY
)\")")"
UNWRAP_REQUEST_IDS_OUT="$(wait_until "unwrap request ids" "blob" icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${EVM_CANISTER_ID}" get_unwrap_request_ids_by_tx_id "(blob \"$(python - <<PY
hexv = "${UNWRAP_TX_ID_HEX}"
print(''.join(f'\\\\{hexv[i:i+2]}' for i in range(0, len(hexv), 2)))
PY
)\")")"
UNWRAP_REQUEST_ID_HEX="$(extract_first_blob_hex "${UNWRAP_REQUEST_IDS_OUT}")"
DISPATCH_OUT="$(wait_until "dispatch" "status = variant { Dispatched }" icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${EVM_CANISTER_ID}" get_unwrap_dispatch_overview "(blob \"$(python - <<PY
hexv = "${UNWRAP_REQUEST_ID_HEX}"
print(''.join(f'\\\\{hexv[i:i+2]}' for i in range(0, len(hexv), 2)))
PY
)\")")"
UNWRAP_RESULT_OUT="$(wait_until "unwrap_result" "status = variant { Succeeded }" icp canister call -e "${ICP_ENV}" --identity "${ICP_IDENTITY_NAME}" --query "${WRAP_CANISTER_ID}" get_request "(blob \"$(python - <<PY
hexv = "${UNWRAP_REQUEST_ID_HEX}"
print(''.join(f'\\\\{hexv[i:i+2]}' for i in range(0, len(hexv), 2)))
PY
)\")")"
UNWRAP_LEDGER_TX_ID_HEX="$(extract_named_blob_hex "${UNWRAP_RESULT_OUT}" "ledger_tx_id")"

cat > "${REPORT_FILE}" <<EOF
# mainnet wrap/unwrap smoke

- timestamp: ${TIMESTAMP}
- identity: ${ICP_IDENTITY_NAME}
- caller_principal: ${CALLER_PRINCIPAL}
- evm_canister: ${EVM_CANISTER_ID}
- wrap_canister: ${WRAP_CANISTER_ID}
- fee_ledger_canister: ${FEE_LEDGER_CANISTER_ID}
- amount_e8s: ${WRAP_AMOUNT_E8S}
- unwrap_recipient: ${UNWRAP_RECIPIENT_PRINCIPAL}
- balance_before: ${BALANCE_OUT}
- allowance_before: ${ALLOWANCE_OUT}

## wrap

- wrap_nonce: ${WRAP_NONCE}
- wrap_gas_limit: ${WRAP_GAS_LIMIT}
- wrap_quoted_fee_e8s: ${WRAP_QUOTED_FEE_E8S}
- wrap_request_id: 0x${WRAP_REQUEST_ID_HEX}
- charged_fee_e8s: ${WRAP_FEE_E8S}
- mint_tx_id: 0x${MINT_TX_ID_HEX}

## unwrap

- wrapped_token: 0x${WRAPPED_TOKEN_HEX}
- approve_estimate_gas: ${APPROVE_ESTIMATE_GAS}
- approve_gas_limit: ${APPROVE_GAS_LIMIT}
- approve_nonce: ${APPROVE_NONCE}
- approve_tx_id: 0x${APPROVE_TX_ID_HEX}
- unwrap_nonce: ${UNWRAP_NONCE}
- unwrap_estimate_gas: ${UNWRAP_ESTIMATE_GAS}
- unwrap_gas_limit: ${UNWRAP_GAS_LIMIT}
- unwrap_tx_id: 0x${UNWRAP_TX_ID_HEX}
- unwrap_request_id: 0x${UNWRAP_REQUEST_ID_HEX}
- unwrap_ledger_tx_id: 0x${UNWRAP_LEDGER_TX_ID_HEX}
EOF

log "report=${REPORT_FILE}"
log "wrap_request_id=0x${WRAP_REQUEST_ID_HEX}"
log "mint_tx_id=0x${MINT_TX_ID_HEX}"
log "unwrap_tx_id=0x${UNWRAP_TX_ID_HEX}"
log "unwrap_request_id=0x${UNWRAP_REQUEST_ID_HEX}"
