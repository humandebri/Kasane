#!/usr/bin/env bash
# where: foundry smoke
# what: perform a minimal gateway compatibility check via cast
# why: provide a manual compatibility check for foundry users
set -euo pipefail

RPC_URL="${EVM_RPC_URL:-http://127.0.0.1:8545}"
ZERO_ADDR="0x0000000000000000000000000000000000000000"
ZERO_SLOT="0x0000000000000000000000000000000000000000000000000000000000000000"

if ! command -v cast >/dev/null 2>&1; then
  echo "[smoke:foundry] SKIP: cast is not installed"
  exit 0
fi

echo "[smoke:foundry] chain-id"
cast chain-id --rpc-url "${RPC_URL}"

echo "[smoke:foundry] block-number"
cast block-number --rpc-url "${RPC_URL}"

echo "[smoke:foundry] balance"
cast balance "${ZERO_ADDR}" --rpc-url "${RPC_URL}"

echo "[smoke:foundry] storage"
cast storage "${ZERO_ADDR}" "${ZERO_SLOT}" --rpc-url "${RPC_URL}"

echo "[smoke:foundry] call"
cast rpc eth_call "{\"to\":\"${ZERO_ADDR}\",\"data\":\"0x\"}" latest --rpc-url "${RPC_URL}"

echo "[smoke:foundry] estimate"
cast rpc eth_estimateGas "{\"to\":\"${ZERO_ADDR}\",\"data\":\"0x\"}" --rpc-url "${RPC_URL}"

echo "[smoke:foundry] ok"
