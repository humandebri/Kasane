#!/usr/bin/env bash
# where: local blockscout validation helper
# what: verify rpc-gateway JSON-RPC and blockscout http availability
# why: 公開前に最低限の接続健全性を確認するため
set -euo pipefail

RPC_URL="${RPC_URL:-http://127.0.0.1:8545}"
BLOCKSCOUT_URL="${BLOCKSCOUT_URL:-http://127.0.0.1:4000}"

json_rpc() {
  local method="$1"
  local params="${2:-[]}"
  curl -sS -X POST "${RPC_URL}" -H 'content-type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"${method}\",\"params\":${params}}"
}

echo "[validate] rpc eth_blockNumber"
json_rpc "eth_blockNumber" | tee /tmp/blockscout-validate-rpc-head.json >/dev/null

echo "[validate] rpc eth_getBlockByNumber(latest,false)"
json_rpc "eth_getBlockByNumber" '["latest",false]' | tee /tmp/blockscout-validate-rpc-block.json >/dev/null

echo "[validate] blockscout http"
curl -fsS "${BLOCKSCOUT_URL}" >/tmp/blockscout-validate-home.html

echo "[validate] ok rpc_url=${RPC_URL} blockscout_url=${BLOCKSCOUT_URL}"
