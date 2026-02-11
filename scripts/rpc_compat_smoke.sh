#!/usr/bin/env bash
# where: RPC compat manual smoke
# what: call rpc_eth_* methods via dfx
# why: keep a low-cost manual check without pocket-ic linking issues
set -euo pipefail

NETWORK="${NETWORK:-local}"
CANISTER_NAME="${CANISTER_NAME:-evm_canister}"
CANISTER_ID="${CANISTER_ID:-}"

DFX="dfx --network ${NETWORK}"

resolve_canister() {
  if [[ -n "${CANISTER_ID}" ]]; then
    echo "${CANISTER_ID}"
  else
    echo "${CANISTER_NAME}"
  fi
}

TARGET=$(resolve_canister)

echo "[rpc-smoke] chain_id"
${DFX} canister call "${TARGET}" rpc_eth_chain_id '( )'

echo "[rpc-smoke] block_number"
${DFX} canister call "${TARGET}" rpc_eth_block_number '( )'

echo "[rpc-smoke] get_block_by_number(0,false)"
${DFX} canister call "${TARGET}" rpc_eth_get_block_by_number '(0, false)'

echo "[rpc-smoke] get_balance(0x0000...)"
${DFX} canister call "${TARGET}" rpc_eth_get_balance '(blob "\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00")'

echo "[rpc-smoke] get_code(0x0000...)"
${DFX} canister call "${TARGET}" rpc_eth_get_code '(blob "\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00\00")'

echo "[rpc-smoke] get_logs_paged(default filter)"
${DFX} canister call "${TARGET}" rpc_eth_get_logs_paged '(record { from_block = null; to_block = null; address = null; topic0 = null; topic1 = null; limit = opt 10 }, null, 10:nat32)'

echo "[rpc-smoke] eth_call_rawtx(empty payload; expect Err)"
${DFX} canister call "${TARGET}" rpc_eth_call_rawtx '(blob "")'
