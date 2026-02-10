#!/usr/bin/env bash
# where: foundry smoke
# what: cast経由でgateway互換を最小確認
# why: foundry利用者向けの手動互換チェックを確保するため
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
cast call "${ZERO_ADDR}" "0x" --rpc-url "${RPC_URL}"

echo "[smoke:foundry] estimate"
cast estimate "${ZERO_ADDR}" "0x" --rpc-url "${RPC_URL}"

echo "[smoke:foundry] ok"
