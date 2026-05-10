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

if [[ "${out}" != *"wrap_factory_address = blob"* ]]; then
  fail "missing wrap_factory_address in init args: ${out}"
fi

if [[ "${out}" != *"wrap_config = null"* ]]; then
  fail "missing default wrap_config in init args: ${out}"
fi

if [[ "${out}" != *"query_instruction_soft_limit = null"* ]]; then
  fail "missing default query_instruction_soft_limit in init args: ${out}"
fi

if [[ "${out}" != *"update_instruction_soft_limit = null"* ]]; then
  fail "missing default update_instruction_soft_limit in init args: ${out}"
fi

custom_out="$(
  WRAP_CANISTER_ID="rrkah-fqaaa-aaaaa-aaaaq-cai" \
  EVM_WRAP_FACTORY="0x1111111111111111111111111111111111111111" \
  QUERY_INSTRUCTION_SOFT_LIMIT="123" \
  UPDATE_INSTRUCTION_SOFT_LIMIT="456" \
  bash -lc 'source "'"${REPO_ROOT}"'/scripts/lib_init_args.sh"; build_init_args_for_principal "'"${principal}"'" "'"${amount}"'"'
)"

if [[ "${custom_out}" != *"query_instruction_soft_limit = opt 123 : nat64"* ]]; then
  fail "missing custom query_instruction_soft_limit in init args: ${custom_out}"
fi

if [[ "${custom_out}" != *"update_instruction_soft_limit = opt 456 : nat64"* ]]; then
  fail "missing custom update_instruction_soft_limit in init args: ${custom_out}"
fi

wrap_config_out="$(
  WRAP_CANISTER_ID="rrkah-fqaaa-aaaaa-aaaaq-cai" \
  EVM_WRAP_FACTORY="0x1111111111111111111111111111111111111111" \
  WRAP_FEE_LEDGER_CANISTER_ID="ryjl3-tyaaa-aaaaa-aaaba-cai" \
  WRAP_NATIVE_LEDGER_CANISTER_ID="r7inp-6aaaa-aaaaa-aaabq-cai" \
  WRAP_ALLOWED_ASSET_IDS="ryjl3-tyaaa-aaaaa-aaaba-cai" \
  WRAP_CYCLE_FEE_E8S="789" \
  WRAP_GAS_PRICE_BUFFER_BPS="12345" \
  bash -lc 'source "'"${REPO_ROOT}"'/scripts/lib_init_args.sh"; build_init_args_for_principal "'"${principal}"'" "'"${amount}"'"'
)"

if [[ "${wrap_config_out}" != *"wrap_config = opt record"* ]]; then
  fail "missing wrap_config record in init args: ${wrap_config_out}"
fi

if [[ "${wrap_config_out}" != *"fee_ledger_canister = principal \"ryjl3-tyaaa-aaaaa-aaaba-cai\""* ]]; then
  fail "missing fee ledger in wrap_config: ${wrap_config_out}"
fi

if [[ "${wrap_config_out}" != *"cycle_fee_e8s = 789 : nat64"* ]]; then
  fail "missing cycle fee in wrap_config: ${wrap_config_out}"
fi

if [[ "${wrap_config_out}" != *"gas_price_buffer_bps = 12345 : nat32"* ]]; then
  fail "missing gas buffer in wrap_config: ${wrap_config_out}"
fi

echo "[test-lib-init-args] ok"
