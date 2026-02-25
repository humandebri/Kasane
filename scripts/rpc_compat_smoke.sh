#!/usr/bin/env bash
# where: RPC compat manual smoke
# what: call rpc_eth_* methods via dfx
# why: keep a low-cost manual check without pocket-ic linking issues
set -euo pipefail

NETWORK="${NETWORK:-local}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
CANISTER_ID="${CANISTER_ID:-}"
export NO_COLOR=1
export CLICOLOR=0
if [[ -z "${TERM:-}" || "${TERM}" == "dumb" ]]; then
  export TERM="xterm-256color"
fi

resolve_canister() {
  if [[ -n "${CANISTER_ID}" ]]; then
    echo "${CANISTER_ID}"
  else
    echo "${CANISTER_NAME}"
  fi
}

TARGET=$(resolve_canister)

dfx_query() {
  dfx canister call --query "$@" --network "${NETWORK}"
}

echo "[rpc-smoke] chain_id"
dfx_query "${TARGET}" rpc_eth_chain_id '( )'

echo "[rpc-smoke] block_number"
dfx_query "${TARGET}" rpc_eth_block_number '( )'

echo "[rpc-smoke] get_block_by_number(0,false)"
dfx_query "${TARGET}" rpc_eth_get_block_by_number '(0, false)'

echo "[rpc-smoke] get_balance(0x0000...)"
dfx_query "${TARGET}" rpc_eth_get_balance '(blob "\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00", variant { Latest })'

echo "[rpc-smoke] get_code(0x0000...)"
dfx_query "${TARGET}" rpc_eth_get_code '(blob "\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00", variant { Latest })'

echo "[rpc-smoke] get_logs_paged(default filter)"
dfx_query "${TARGET}" rpc_eth_get_logs_paged '(record { from_block = null; to_block = null; address = null; topic0 = null; topic1 = null; limit = opt 10 }, null, 10:nat32)'

echo "[rpc-smoke] eth_call_rawtx(empty payload; expect Err)"
dfx_query "${TARGET}" rpc_eth_call_rawtx '(blob "")'
