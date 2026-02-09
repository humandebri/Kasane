#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

if ! command -v canbench >/dev/null 2>&1; then
  echo "[canbench-guard] installing canbench"
  cargo install --locked canbench
fi

# Prefer a local PocketIC binary to avoid flaky network downloads during canbench startup.
if [[ -z "${POCKET_IC_BIN:-}" ]]; then
  if [[ -x "${ROOT_DIR}/crates/evm-rpc-e2e/pocket-ic" ]]; then
    export POCKET_IC_BIN="${ROOT_DIR}/crates/evm-rpc-e2e/pocket-ic"
    echo "[canbench-guard] using local PocketIC binary: ${POCKET_IC_BIN}"
  elif [[ -x "${ROOT_DIR}/pocket-ic" ]]; then
    export POCKET_IC_BIN="${ROOT_DIR}/pocket-ic"
    echo "[canbench-guard] using local PocketIC binary: ${POCKET_IC_BIN}"
  fi
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
    echo "[canbench-guard] WARN: baseline not found on origin/${BASE_REF}; using working tree baseline (${BASELINE_FILE})"
    echo "[canbench-guard] WARN: this is expected on initial canbench baseline introduction PRs"
  else
    echo "[canbench-guard] WARN: fallback to working tree baseline (${BASELINE_FILE})"
  fi
  cp "$BASELINE_FILE" "$tmp_baseline"
fi

# canbench --persist updates canbench_results.yml with current measurements.
# Retry to reduce transient startup/network flakes from underlying PocketIC bootstrapping.
attempt=1
max_attempts=3
while true; do
  if canbench --persist; then
    break
  fi
  if [[ "$attempt" -ge "$max_attempts" ]]; then
    echo "[canbench-guard] ERROR: canbench failed after ${max_attempts} attempts" >&2
    exit 1
  fi
  sleep_sec=$((attempt * 3))
  echo "[canbench-guard] WARN: canbench attempt ${attempt} failed, retrying in ${sleep_sec}s" >&2
  sleep "${sleep_sec}"
  attempt=$((attempt + 1))
done

scripts/check_canbench_thresholds.sh "$tmp_baseline" "$BASELINE_FILE"

echo "[canbench-guard] baseline updated: $BASELINE_FILE"
