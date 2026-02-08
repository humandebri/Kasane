#!/usr/bin/env bash
# どこで: workspace全体（vendor含む） / 何を: 直接RNG呼び出しの混入検出 / なぜ: wasmでOS RNG経路を踏まないため

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

PATTERN='OsRng|thread_rng|getrandom::|rand::rngs'
TMP_FILE="$(mktemp)"
trap 'rm -f "$TMP_FILE"' EXIT

if command -v rg >/dev/null 2>&1; then
  rg -n "$PATTERN" . \
    --glob '!target/**' \
    --glob '!docs/**' \
    --glob '!scripts/**' \
    --glob '!vendor/**/CHANGELOG.md' \
    > "$TMP_FILE" || true
else
  grep -RInE "$PATTERN" . \
    --exclude-dir=.git \
    --exclude-dir=target \
    --exclude-dir=docs \
    --exclude-dir=scripts \
    --exclude='CHANGELOG.md' \
    > "$TMP_FILE" || true
fi

if [[ -s "$TMP_FILE" ]]; then
  # 許可: wasmでcustom backendを登録する箇所のみ
  grep -v '^\.\/crates\/ic-evm-wrapper\/src\/lib.rs:' "$TMP_FILE" > "$TMP_FILE.filtered" || true
  mv "$TMP_FILE.filtered" "$TMP_FILE"
fi

if [[ -s "$TMP_FILE" ]]; then
  echo "ERROR: direct RNG/getrandom callsites detected:"
  cat "$TMP_FILE"
  exit 1
fi

echo "OK: no direct RNG/getrandom callsites (allowlist-only)."
