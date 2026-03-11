#!/usr/bin/env bash
# where: script-level tests for init arg helpers
# what: verify init args encode only genesis balances
# why: wrap canister id is runtime default and no longer required at install time
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

fail() {
  echo "[test-lib-init-args] FAIL: $*" >&2
  exit 1
}

principal="aaaaa-aa"
amount="123"
out="$(
  bash -lc 'source "'"${REPO_ROOT}"'/scripts/lib_init_args.sh"; build_init_args_for_principal "'"${principal}"'" "'"${amount}"'"'
)"

if [[ "${out}" != *"genesis_balances = vec"* ]]; then
  fail "missing genesis_balances in init args: ${out}"
fi

if [[ "${out}" == *"wrap_canister_id"* ]]; then
  fail "wrap_canister_id should not be present in init args: ${out}"
fi

echo "[test-lib-init-args] ok"
