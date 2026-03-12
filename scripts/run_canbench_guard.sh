#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

REQUIRED_CANBENCH_VERSION="0.4.1"
CANBENCH_ROOT="${ROOT_DIR}/.canbench-tools"
CANBENCH_BIN="${CANBENCH_ROOT}/bin/canbench"
installed_canbench_version=""
if [[ -x "${CANBENCH_BIN}" ]]; then
  installed_canbench_version="$("${CANBENCH_BIN}" --version 2>/dev/null | awk '{print $2}')"
fi

if [[ "${installed_canbench_version}" != "${REQUIRED_CANBENCH_VERSION}" ]]; then
  echo "[canbench-guard] installing canbench ${REQUIRED_CANBENCH_VERSION} into ${CANBENCH_ROOT}"
  cargo install \
    --root "${CANBENCH_ROOT}" \
    --locked \
    --force \
    canbench \
    --version "${REQUIRED_CANBENCH_VERSION}"
fi

# canbench uses --runtime-path to choose the PocketIC binary.
declare -a CANBENCH_ARGS=()
if [[ -n "${POCKET_IC_BIN:-}" ]]; then
  if [[ -x "${POCKET_IC_BIN}" ]]; then
    echo "[canbench-guard] using PocketIC binary from POCKET_IC_BIN: ${POCKET_IC_BIN}"
    CANBENCH_ARGS+=(--runtime-path "${POCKET_IC_BIN}")
  else
    echo "[canbench-guard] WARN: POCKET_IC_BIN is not executable: ${POCKET_IC_BIN}" >&2
  fi
else
  for candidate in \
    "${ROOT_DIR}/.canbench/pocket-ic" \
    "${ROOT_DIR}/pocket-ic" \
    "${ROOT_DIR}/crates/evm-rpc-e2e/pocket-ic"
  do
    if [[ -x "${candidate}" ]]; then
      echo "[canbench-guard] using PocketIC binary: ${candidate}"
      CANBENCH_ARGS+=(--runtime-path "${candidate}")
      break
    fi
  done
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
  if "${CANBENCH_BIN}" --persist "${CANBENCH_ARGS[@]}"; then
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
