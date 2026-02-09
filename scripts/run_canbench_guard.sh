#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

if ! command -v canbench >/dev/null 2>&1; then
  echo "[canbench-guard] installing canbench"
  cargo install --locked canbench
fi

BASELINE_FILE="canbench_results.yml"
if [[ ! -f "$BASELINE_FILE" ]]; then
  echo "[canbench-guard] baseline file not found: $BASELINE_FILE"
  exit 2
fi

tmp_baseline="$(mktemp)"
BASE_REF="${GITHUB_BASE_REF:-main}"
if git show "origin/${BASE_REF}:${BASELINE_FILE}" >"$tmp_baseline" 2>/dev/null; then
  echo "[canbench-guard] baseline source: origin/${BASE_REF}:${BASELINE_FILE}"
else
  if [[ "${CI:-}" == "true" || "${GITHUB_ACTIONS:-}" == "true" ]]; then
    echo "[canbench-guard] ERROR: failed to read baseline from origin/${BASE_REF}:${BASELINE_FILE}" >&2
    exit 1
  fi
  echo "[canbench-guard] WARN: fallback to working tree baseline (${BASELINE_FILE})"
  cp "$BASELINE_FILE" "$tmp_baseline"
fi

# canbench --persist updates canbench_results.yml with current measurements.
canbench --persist

scripts/check_canbench_thresholds.sh "$tmp_baseline" "$BASELINE_FILE"

echo "[canbench-guard] baseline updated: $BASELINE_FILE"
