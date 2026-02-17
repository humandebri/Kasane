#!/usr/bin/env bash
# where: rpc-gateway ops
# what: tx hash から receipt-watch@.service を起動する
# why: 送信成功と実行成功の監視導線を常に同じ起動元に固定するため
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "usage: $0 <tx_hash> [status_lines=40]" >&2
  exit 2
fi

TX_HASH_RAW="$1"
STATUS_LINES="${2:-40}"
TX_HASH="${TX_HASH_RAW,,}"

if ! [[ "$TX_HASH" =~ ^0x[0-9a-f]{64}$ ]]; then
  echo "invalid tx hash: $TX_HASH_RAW" >&2
  exit 2
fi

if ! [[ "$STATUS_LINES" =~ ^[0-9]+$ ]]; then
  echo "status_lines must be an integer: $STATUS_LINES" >&2
  exit 2
fi

UNIT="receipt-watch@${TX_HASH}.service"
systemctl start "$UNIT"
systemctl status --no-pager "$UNIT" | sed -n "1,${STATUS_LINES}p"
