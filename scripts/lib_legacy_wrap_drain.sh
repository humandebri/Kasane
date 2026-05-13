#!/usr/bin/env bash
# where: mainnet preflight helpers
# what: validate legacy standalone wrap canister requests are drained
# why: integrated wrap upgrade intentionally does not keep legacy caller compatibility

legacy_wrap_log() {
  echo "[legacy-wrap-drain] $*"
}

legacy_wrap_trim() {
  local value="$1"
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"
  printf '%s' "${value}"
}

legacy_wrap_drain_required() {
  if [[ -z "${LEGACY_WRAP_CANISTER_ID:-}" ]]; then
    return 1
  fi
  local integrated="${EVM_CANISTER_ID:-${CANISTER_ID:-}}"
  [[ -z "${integrated}" || "${LEGACY_WRAP_CANISTER_ID}" != "${integrated}" ]]
}

legacy_wrap_candid_arg_for_request_id() {
  local raw
  raw="$(legacy_wrap_trim "$1")"
  if [[ "${raw}" == \(* ]]; then
    printf '%s' "${raw}"
    return 0
  fi
  if [[ "${raw}" == blob\ * ]]; then
    printf '(%s)' "${raw}"
    return 0
  fi
  RAW_REQUEST_ID="${raw}" python - <<'PY'
import os
import re
raw = os.environ["RAW_REQUEST_ID"].strip()
if raw.startswith("0x"):
    raw = raw[2:]
if not re.fullmatch(r"[0-9a-fA-F]{64}", raw):
    raise SystemExit(f"invalid legacy wrap request id: {os.environ['RAW_REQUEST_ID']}")
escaped = "".join(f"\\{raw[i:i+2].lower()}" for i in range(0, len(raw), 2))
print(f'(blob "{escaped}")')
PY
}

legacy_wrap_query() {
  local method="$1"
  local arg="$2"
  local dfx_bin="${DFX_BIN:-dfx}"
  local network="${ICP_ENV:-ic}"
  local candid="${LEGACY_WRAP_CANISTER_DID:-crates/ic-evm-gateway/evm_canister.did}"
  local cmd=("${dfx_bin}" canister call --query --network "${network}" --output pp)
  if [[ -n "${ICP_IDENTITY_NAME:-}" ]]; then
    cmd+=(--identity "${ICP_IDENTITY_NAME}")
  fi
  if [[ -n "${candid}" ]]; then
    cmd+=(--candid "${candid}")
  fi
  cmd+=("${LEGACY_WRAP_CANISTER_ID}" "${method}" "${arg}")
  "${cmd[@]}"
}

legacy_wrap_output_state() {
  local output="$1"
  local status=""
  local dispatch=""
  if [[ "${output}" =~ dispatch_status[[:space:]]*=[[:space:]]*opt[[:space:]]+variant[[:space:]]*\{[[:space:]]*([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*\} ]]; then
    dispatch="${BASH_REMATCH[1]}"
  fi
  case "${dispatch}" in
    Queued|Dispatching|DispatchFailed)
      echo "active"
      return 0
      ;;
  esac
  if [[ "${output}" =~ status[[:space:]]*=[[:space:]]*variant[[:space:]]*\{[[:space:]]*([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*\} ]]; then
    status="${BASH_REMATCH[1]}"
  fi
  case "${status}" in
    Succeeded|Failed)
      echo "terminal"
      ;;
    Queued|Running)
      echo "active"
      ;;
    "")
      if [[ "${output}" == *"null"* ]]; then
        echo "missing"
      else
        echo "unknown"
      fi
      ;;
    *)
      echo "unknown"
      ;;
  esac
}

legacy_wrap_check_request_id() {
  local request_id="$1"
  local arg
  arg="$(legacy_wrap_candid_arg_for_request_id "${request_id}")"

  local out state
  out="$(legacy_wrap_query get_request "${arg}")"
  state="$(legacy_wrap_output_state "${out}")"
  case "${state}" in
    terminal)
      legacy_wrap_log "terminal get_request ${request_id}"
      return 0
      ;;
    active)
      echo "[legacy-wrap-drain] active legacy request in get_request: ${request_id}" >&2
      return 1
      ;;
    unknown)
      echo "[legacy-wrap-drain] unparseable get_request response for ${request_id}: ${out}" >&2
      return 1
      ;;
  esac

  out="$(legacy_wrap_query get_native_deposit_result "${arg}")"
  state="$(legacy_wrap_output_state "${out}")"
  case "${state}" in
    terminal)
      legacy_wrap_log "terminal get_native_deposit_result ${request_id}"
      ;;
    active)
      echo "[legacy-wrap-drain] active legacy request in get_native_deposit_result: ${request_id}" >&2
      return 1
      ;;
    missing)
      echo "[legacy-wrap-drain] legacy request missing in both views: ${request_id}" >&2
      return 1
      ;;
    *)
      echo "[legacy-wrap-drain] unparseable get_native_deposit_result response for ${request_id}: ${out}" >&2
      return 1
      ;;
  esac
}

check_legacy_wrap_drain() {
  if ! legacy_wrap_drain_required; then
    legacy_wrap_log "skip"
    return 0
  fi
  if [[ -z "${LEGACY_WRAP_REQUEST_IDS_FILE:-}" ]]; then
    echo "[legacy-wrap-drain] LEGACY_WRAP_REQUEST_IDS_FILE is required" >&2
    return 1
  fi
  if [[ ! -f "${LEGACY_WRAP_REQUEST_IDS_FILE}" ]]; then
    echo "[legacy-wrap-drain] request id file not found: ${LEGACY_WRAP_REQUEST_IDS_FILE}" >&2
    return 1
  fi

  local count=0
  local raw line
  while IFS= read -r raw || [[ -n "${raw}" ]]; do
    line="$(legacy_wrap_trim "${raw}")"
    [[ -z "${line}" || "${line}" == \#* ]] && continue
    count=$((count + 1))
    legacy_wrap_check_request_id "${line}" || return 1
  done < "${LEGACY_WRAP_REQUEST_IDS_FILE}"

  if [[ "${count}" -eq 0 ]]; then
    if [[ "${ALLOW_EMPTY_LEGACY_WRAP_REQUESTS:-0}" != "1" ]]; then
      echo "[legacy-wrap-drain] empty request id file requires ALLOW_EMPTY_LEGACY_WRAP_REQUESTS=1" >&2
      return 1
    fi
    legacy_wrap_log "empty request manifest accepted by explicit attestation"
  fi
  legacy_wrap_log "passed"
}
