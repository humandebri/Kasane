#!/usr/bin/env bash
# where: script-level tests for init arg helpers
# what: verify init args encode runtime config as well as genesis balances
# why: gateway install/upgrade now requires explicit wrap settings
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

fail() {
  echo "[test-lib-init-args] FAIL: $*" >&2
  exit 1
}

principal="aaaaa-aa"
amount="123"
out="$(
  WRAP_CANISTER_ID="rrkah-fqaaa-aaaaa-aaaaq-cai" EVM_WRAP_FACTORY="0x1111111111111111111111111111111111111111" \
  bash -lc 'source "'"${REPO_ROOT}"'/scripts/lib_init_args.sh"; build_init_args_for_principal "'"${principal}"'" "'"${amount}"'"'
)"

if [[ "${out}" != *"genesis_balances = vec"* ]]; then
  fail "missing genesis_balances in init args: ${out}"
fi

if [[ "${out}" != *"wrap_canister_id = principal"* ]]; then
  fail "missing wrap_canister_id in init args: ${out}"
fi

if [[ "${out}" != *"wrap_factory_address = vec"* ]]; then
  fail "missing wrap_factory_address in init args: ${out}"
fi

echo "[test-lib-init-args] ok"
