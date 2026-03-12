#!/usr/bin/env bash
# where: deploy/install scripts shared helper
# what: build mandatory InitArgs candid text for ic-evm-gateway install/upgrade
# why: keep runtime config explicit across every deploy path

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEFAULT_GENESIS_AMOUNT="100000000000000000000000"

validate_init_args_text() {
  local text="$1"
  if [[ -z "${text}" ]]; then
    echo "[lib_init_args] error: init args is empty" >&2
    return 1
  fi
  if [[ "${text}" != *"(opt record"* ]]; then
    echo "[lib_init_args] error: init args must include '(opt record'" >&2
    return 1
  fi
}

require_wrap_runtime_config() {
  if [[ -z "${WRAP_CANISTER_ID:-}" ]]; then
    echo "[lib_init_args] error: WRAP_CANISTER_ID is required" >&2
    return 1
  fi
  if [[ -z "${EVM_WRAP_FACTORY:-}" ]]; then
    echo "[lib_init_args] error: EVM_WRAP_FACTORY is required" >&2
    return 1
  fi
}

caller_evm_blob_from_principal() {
  local principal="$1"
  local caller_hex
  caller_hex=$(cargo run -q --manifest-path "${REPO_ROOT}/Cargo.toml" -p ic-evm-core --bin derive_evm_address -- "${principal}")
  python - <<PY
hex_str = "${caller_hex}".strip()
data = bytes.fromhex(hex_str)
print(''.join(f'\\\\{b:02x}' for b in data))
PY
}

build_init_args_for_principal() {
  local principal="$1"
  local amount="${2:-$DEFAULT_GENESIS_AMOUNT}"
  require_wrap_runtime_config
  local blob
  blob=$(caller_evm_blob_from_principal "${principal}")
  local factory_blob
  factory_blob="$(
    EVM_WRAP_FACTORY="${EVM_WRAP_FACTORY}" python - <<'PY'
import os

value = os.environ["EVM_WRAP_FACTORY"].strip()
if value.startswith("0x"):
    value = value[2:]
if len(value) != 40:
    raise SystemExit("EVM_WRAP_FACTORY must be 20 bytes")
raw = bytes.fromhex(value)
print(''.join(f'\\{byte:02x}' for byte in raw))
PY
  )"
  local out
  out=$(cat <<EOF
(opt record {
  genesis_balances = vec { record { address = blob "${blob}"; amount = ${amount} : nat } };
  wrap_canister_id = principal "${WRAP_CANISTER_ID}";
  wrap_factory_address = blob "${factory_blob}";
})
EOF
)
  validate_init_args_text "${out}"
  printf '%s\n' "${out}"
}

build_init_args_for_current_identity() {
  local amount="${1:-$DEFAULT_GENESIS_AMOUNT}"
  require_wrap_runtime_config
  local principal
  if command -v icp >/dev/null 2>&1; then
    if [[ -n "${ICP_IDENTITY_NAME:-}" ]]; then
      principal="$(icp identity principal --identity "${ICP_IDENTITY_NAME}")"
    else
      principal="$(icp identity principal)"
    fi
  else
    echo "[lib_init_args] error: icp command is required to resolve current identity principal" >&2
    return 1
  fi
  local out
  out="$(build_init_args_for_principal "${principal}" "${amount}")"
  validate_init_args_text "${out}"
  printf '%s\n' "${out}"
}
