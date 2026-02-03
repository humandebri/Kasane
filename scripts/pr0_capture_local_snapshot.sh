#!/usr/bin/env bash
# どこで: PR0差分テスト基盤 / 何を: ローカル実行結果から比較用スナップショットを抽出 / なぜ: CIで常時差分検知を有効にするため

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: scripts/pr0_capture_local_snapshot.sh <output_file>" >&2
  exit 1
fi

OUT_FILE="$1"
TMP_FILE="$(mktemp)"
trap 'rm -f "$TMP_FILE"' EXIT

cargo test -p ic-evm-core --test pr0_snapshots -- --nocapture >"$TMP_FILE" 2>&1

{
  grep "^SNAPSHOT_TX_MATRIX:" "$TMP_FILE" | head -n 1
  grep "^SNAPSHOT_BLOCK:" "$TMP_FILE" | head -n 1
} >"$OUT_FILE"

if [[ ! -s "$OUT_FILE" ]]; then
  echo "failed to capture PR0 snapshot lines" >&2
  cat "$TMP_FILE" >&2
  exit 1
fi

echo "OK: captured PR0 snapshot to $OUT_FILE"
