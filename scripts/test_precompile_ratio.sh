#!/usr/bin/env bash
# where: script-level test for precompile ratio measurement helper
# what: verify clear -> measure -> suggest flow with mocked dfx/workload
# why: keep the operator workflow stable without depending on a live canister
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

MOCK_DFX="${TMP_DIR}/dfx"
MOCK_WORKLOAD="${TMP_DIR}/workload.sh"
CALLS_LOG="${TMP_DIR}/calls.log"
REPORT_JSON="${TMP_DIR}/report.json"
STDERR_LOG="${TMP_DIR}/stderr.log"

cat > "${MOCK_DFX}" <<'SCRIPT'
#!/usr/bin/env bash
set -euo pipefail
echo "$*" >> "${MOCK_CALLS_LOG}"

if printf '%s' "$*" | grep -q "clear_precompile_profile"; then
  if [[ "${MOCK_CLEAR_MODE:-ok}" == "err" ]]; then
    printf '%s\n' '(variant { Err = "auth.controller_required" })'
    exit 0
  fi
  if [[ "${MOCK_CLEAR_MODE:-ok}" == "transport_fail" ]]; then
    echo "transport error" >&2
    exit 2
  fi
  printf '%s\n' '(variant { Ok = null })'
  exit 0
fi

if printf '%s' "$*" | grep -q "get_precompile_profile"; then
  python - <<'PY'
import json
print(json.dumps([
    {
        "address": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
        "calls": 30,
        "total_instructions": 900000,
        "avg_instructions": 30000,
        "max_instructions": 31000,
        "total_extra_gas": 9000,
        "avg_extra_gas": 300,
        "max_extra_gas": 310
    },
    {
        "address": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6],
        "calls": 30,
        "total_instructions": 45000,
        "avg_instructions": 1500,
        "max_instructions": 1600,
        "total_extra_gas": 450,
        "avg_extra_gas": 15,
        "max_extra_gas": 16
    }
]))
PY
  exit 0
fi

printf '%s\n' '(variant { Ok = null })'
SCRIPT

cat > "${MOCK_WORKLOAD}" <<'SCRIPT'
#!/usr/bin/env bash
set -euo pipefail
echo "${WORKLOAD_ITERATION:-0}" >> "${MOCK_WORKLOAD_LOG}"
SCRIPT

chmod +x "${MOCK_DFX}" "${MOCK_WORKLOAD}"

fail() {
  echo "[test-precompile-ratio] FAIL: $*" >&2
  exit 1
}

assert_grep() {
  local pattern="$1"
  local file="$2"
  if ! grep -q -- "${pattern}" "${file}"; then
    fail "pattern not found: ${pattern} in ${file}"
  fi
}

assert_not_grep() {
  local pattern="$1"
  local file="$2"
  if grep -q -- "${pattern}" "${file}"; then
    fail "unexpected pattern found: ${pattern} in ${file}"
  fi
}

run_measure() {
  MOCK_CALLS_LOG="${CALLS_LOG}" \
  MOCK_WORKLOAD_LOG="${TMP_DIR}/workload.log" \
  DFX_BIN="${MOCK_DFX}" \
  CANISTER_NAME_OR_ID="abcd" \
  NETWORK="local" \
  WORKLOAD_CMD="${MOCK_WORKLOAD}" \
  WORKLOAD_RUNS="30" \
  MIN_CALLS="30" \
  REFERENCE_PRECOMPILE_ADDRESS="0x0000000000000000000000000000000000000001" \
  REFERENCE_TARGET_GAS="3000" \
  SAFETY_MULTIPLIER="1.00" \
  REPORT_JSON_PATH="${REPORT_JSON}" \
  "${REPO_ROOT}/scripts/measure_precompile_ratio.sh"
}

run_measure >/dev/null

assert_grep "clear_precompile_profile ()" "${CALLS_LOG}"
assert_grep "get_precompile_profile () --output json" "${CALLS_LOG}"

python - <<'PY' "${REPORT_JSON}" "${TMP_DIR}/workload.log"
import json
import sys

report_path, workload_log_path = sys.argv[1:3]
with open(report_path, "r", encoding="utf-8") as f:
    report = json.load(f)

ratio = report["recommendation"]
if ratio["numerator"] != 1 or ratio["denominator"] != 10:
    raise SystemExit("unexpected suggested ratio")

qualifying = [item for item in report["measurement"]["entries"] if item["qualifies"]]
if len(qualifying) != 2:
    raise SystemExit("expected two qualifying entries")
if any(item["avg_extra_gas"] <= 0 for item in qualifying):
    raise SystemExit("expected positive measured extra gas")

with open(workload_log_path, "r", encoding="utf-8") as f:
    runs = [line.strip() for line in f if line.strip()]
if len(runs) != 30:
    raise SystemExit("expected workload runs")
PY

: > "${CALLS_LOG}"
rm -f "${TMP_DIR}/workload.log" "${REPORT_JSON}" "${STDERR_LOG}"
if MOCK_CLEAR_MODE="err" run_measure >/dev/null 2>"${STDERR_LOG}"; then
  fail "expected canister Err path to fail"
fi
assert_grep "clear_precompile_profile ()" "${CALLS_LOG}"
assert_not_grep "get_precompile_profile () --output json" "${CALLS_LOG}"
assert_grep "clear_precompile_profile failed" "${STDERR_LOG}"
assert_grep "auth.controller_required" "${STDERR_LOG}"
if [[ -f "${TMP_DIR}/workload.log" ]] && [[ -s "${TMP_DIR}/workload.log" ]]; then
  fail "workload must not run when clear returns Err"
fi

: > "${CALLS_LOG}"
rm -f "${TMP_DIR}/workload.log" "${REPORT_JSON}" "${STDERR_LOG}"
if MOCK_CLEAR_MODE="transport_fail" run_measure >/dev/null 2>"${STDERR_LOG}"; then
  fail "expected transport failure path to fail"
fi
assert_grep "clear_precompile_profile ()" "${CALLS_LOG}"
assert_not_grep "get_precompile_profile () --output json" "${CALLS_LOG}"
assert_grep "transport error" "${STDERR_LOG}"
if [[ -f "${TMP_DIR}/workload.log" ]] && [[ -s "${TMP_DIR}/workload.log" ]]; then
  fail "workload must not run on transport failure"
fi

echo "[test-precompile-ratio] ok"
