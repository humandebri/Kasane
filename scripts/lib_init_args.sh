#!/usr/bin/env bash
# where: deploy/install scripts shared helper
# what: build mandatory InitArgs candid text for ic-evm-wrapper install
# why: eliminate empty install-arg paths across all environments

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEFAULT_GENESIS_AMOUNT="1000000000000000000"

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

caller_evm_blob_from_principal() {
  local principal="$1"
  local caller_hex
  caller_hex=$(cargo run -q --manifest-path "${REPO_ROOT}/Cargo.toml" -p ic-evm-core --bin caller_evm -- "${principal}")
  python - <<PY
hex_str = "${caller_hex}".strip()
data = bytes.fromhex(hex_str)
print(''.join(f'\\\\{b:02x}' for b in data))
PY
}

build_init_args_for_principal() {
  local principal="$1"
  local amount="${2:-$DEFAULT_GENESIS_AMOUNT}"
  local blob
  blob=$(caller_evm_blob_from_principal "${principal}")
  local out
  out=$(cat <<EOF
(opt record { genesis_balances = vec { record { address = blob "${blob}"; amount = ${amount} : nat } } })
EOF
)
  validate_init_args_text "${out}"
  printf '%s\n' "${out}"
}

build_init_args_for_current_identity() {
  local amount="${1:-$DEFAULT_GENESIS_AMOUNT}"
  local principal
  principal="$(dfx identity get-principal)"
  local out
  out="$(build_init_args_for_principal "${principal}" "${amount}")"
  validate_init_args_text "${out}"
  printf '%s\n' "${out}"
}
