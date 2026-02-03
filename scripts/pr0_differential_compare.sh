#!/usr/bin/env bash
# どこで: PR0のオフチェーン差分検証 / 何を: ローカル出力と参照実装出力の差分比較 / なぜ: 後続PRで意図しないセマンティクス差分を検知するため

set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: scripts/pr0_differential_compare.sh <local_output.txt> <reference_output.txt>" >&2
  exit 1
fi

LOCAL_FILE="$1"
REFERENCE_FILE="$2"

if [[ ! -f "$LOCAL_FILE" ]]; then
  echo "local output not found: $LOCAL_FILE" >&2
  exit 1
fi

if [[ ! -f "$REFERENCE_FILE" ]]; then
  echo "reference output not found: $REFERENCE_FILE" >&2
  exit 1
fi

if diff -u "$REFERENCE_FILE" "$LOCAL_FILE"; then
  echo "OK: PR0 differential check passed."
  exit 0
fi

echo "NG: PR0 differential check failed (see diff above)." >&2
exit 1
