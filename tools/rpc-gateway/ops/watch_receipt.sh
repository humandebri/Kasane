#!/usr/bin/env bash
# where: rpc-gateway ops
# what: tx hash単位でreceipt.status監視を実行し、失敗時は任意Webhookへ通知
# why: submit成功と実行成功を分離し、本番監視へ組み込むため
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <tx_hash> [max_wait_sec=180] [interval_ms=1500]" >&2
  exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GATEWAY_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TX_HASH="$1"
MAX_WAIT_SEC="${2:-180}"
INTERVAL_MS="${3:-1500}"
RPC_URL="${EVM_RPC_URL:-http://127.0.0.1:8545}"
ALERT_WEBHOOK_URL="${ALERT_WEBHOOK_URL:-}"

set +e
OUTPUT="$(
  cd "$GATEWAY_DIR" && \
    EVM_RPC_URL="$RPC_URL" npm run -s smoke:watch-receipt -- "$TX_HASH" "$MAX_WAIT_SEC" "$INTERVAL_MS" 2>&1
)"
RC=$?
set -e

echo "$OUTPUT"

if [[ $RC -eq 0 ]]; then
  exit 0
fi

if [[ -n "$ALERT_WEBHOOK_URL" ]]; then
  BODY="$(printf '{"tx_hash":"%s","rpc_url":"%s","error":"%s"}' "$TX_HASH" "$RPC_URL" "$(echo "$OUTPUT" | tail -n 1 | sed 's/"/\\"/g')")"
  curl -sS -X POST "$ALERT_WEBHOOK_URL" \
    -H "content-type: application/json" \
    --data "$BODY" >/dev/null || true
fi

exit "$RC"
