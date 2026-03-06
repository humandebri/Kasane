#!/usr/bin/env bash
# where: operator-side measurement helper
# what: replay a workload and summarize precompile profile for fixed-ratio calibration
# why: choose or revisit a fixed gas-per-instruction ratio from IC instruction counters instead of host-side wall-clock timings
set -euo pipefail

CANISTER_NAME_OR_ID="${CANISTER_NAME_OR_ID:-}"
NETWORK="${NETWORK:-local}"
DFX_BIN="${DFX_BIN:-dfx}"
WORKLOAD_CMD="${WORKLOAD_CMD:-}"
WORKLOAD_RUNS="${WORKLOAD_RUNS:-30}"
MIN_CALLS="${MIN_CALLS:-30}"
MAX_DENOMINATOR="${MAX_DENOMINATOR:-1000000}"
TARGET_GAS_PER_INSTRUCTION="${TARGET_GAS_PER_INSTRUCTION:-}"
REFERENCE_PRECOMPILE_ADDRESS="${REFERENCE_PRECOMPILE_ADDRESS:-}"
REFERENCE_TARGET_GAS="${REFERENCE_TARGET_GAS:-}"
SAFETY_MULTIPLIER="${SAFETY_MULTIPLIER:-1.10}"
REPORT_JSON_PATH="${REPORT_JSON_PATH:-}"

log() {
  echo "[measure-precompile-ratio] $*"
}

usage() {
  cat <<'USAGE'
usage:
  CANISTER_NAME_OR_ID=<id> \
  WORKLOAD_CMD='scripts/playground_smoke.sh' \
  scripts/measure_precompile_ratio.sh

example with measured reference precompile:
  CANISTER_NAME_OR_ID=<id> \
  WORKLOAD_CMD='PRECOMPILE_PROFILE_TARGETS=modexp_heavy scripts/run_precompile_profile_e2e.sh' \
  REFERENCE_PRECOMPILE_ADDRESS=0x0000000000000000000000000000000000000005 \
  REFERENCE_TARGET_GAS=3000000 \
  scripts/measure_precompile_ratio.sh

optional environment:
  NETWORK=local
  WORKLOAD_RUNS=30
  MIN_CALLS=30
  TARGET_GAS_PER_INSTRUCTION=0.005
  REFERENCE_PRECOMPILE_ADDRESS=0x0000000000000000000000000000000000000001
  REFERENCE_TARGET_GAS=3000
  SAFETY_MULTIPLIER=1.10
  REPORT_JSON_PATH=/tmp/precompile_ratio_report.json

notes:
  - this script no longer mutates canister ratio state.
  - TARGET_GAS_PER_INSTRUCTION is used directly when set.
  - otherwise, if REFERENCE_PRECOMPILE_ADDRESS and REFERENCE_TARGET_GAS are set,
    the suggested fixed ratio is derived from the measured avg_instructions of that precompile.
USAGE
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[measure-precompile-ratio] missing command: $1" >&2
    exit 1
  fi
}

require_non_negative_int() {
  local name="$1"
  local value="$2"
  if [[ ! "${value}" =~ ^[0-9]+$ ]]; then
    echo "[measure-precompile-ratio] ${name} must be a non-negative integer: ${value}" >&2
    exit 1
  fi
}

run_update_call() {
  local method="$1"
  local args="$2"
  local output
  output="$("${DFX_BIN}" canister --network "${NETWORK}" call "${CANISTER_NAME_OR_ID}" "${method}" "${args}")"
  if [[ "${output}" == *"variant { Ok"* ]]; then
    return 0
  fi
  if [[ "${output}" == *"variant { Err"* ]]; then
    echo "[measure-precompile-ratio] ${method} failed: ${output}" >&2
    exit 1
  fi
  echo "[measure-precompile-ratio] ${method} returned unexpected output: ${output}" >&2
  exit 1
}

query_profile_json() {
  "${DFX_BIN}" canister --network "${NETWORK}" call --query "${CANISTER_NAME_OR_ID}" get_precompile_profile '()' --output json
}

run_workload() {
  local runs="$1"
  local i
  for ((i = 1; i <= runs; i += 1)); do
    log "run workload ${i}/${runs}"
    WORKLOAD_ITERATION="${i}" WORKLOAD_RUNS="${runs}" bash -lc "${WORKLOAD_CMD}"
  done
}

if [[ -z "${CANISTER_NAME_OR_ID}" || -z "${WORKLOAD_CMD}" ]]; then
  usage >&2
  exit 1
fi

require_cmd "${DFX_BIN}"
require_cmd bash
require_cmd python
require_non_negative_int "WORKLOAD_RUNS" "${WORKLOAD_RUNS}"
require_non_negative_int "MIN_CALLS" "${MIN_CALLS}"
require_non_negative_int "MAX_DENOMINATOR" "${MAX_DENOMINATOR}"
if [[ -n "${REFERENCE_TARGET_GAS}" ]]; then
  require_non_negative_int "REFERENCE_TARGET_GAS" "${REFERENCE_TARGET_GAS}"
fi
if (( WORKLOAD_RUNS < 1 )); then
  echo "[measure-precompile-ratio] WORKLOAD_RUNS must be >= 1" >&2
  exit 1
fi
if [[ -n "${TARGET_GAS_PER_INSTRUCTION}" && -n "${REFERENCE_PRECOMPILE_ADDRESS}" ]]; then
  echo "[measure-precompile-ratio] set either TARGET_GAS_PER_INSTRUCTION or REFERENCE_PRECOMPILE_ADDRESS/REFERENCE_TARGET_GAS" >&2
  exit 1
fi
if [[ -n "${REFERENCE_PRECOMPILE_ADDRESS}" && -z "${REFERENCE_TARGET_GAS}" ]]; then
  echo "[measure-precompile-ratio] REFERENCE_TARGET_GAS is required when REFERENCE_PRECOMPILE_ADDRESS is set" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT
BASELINE_JSON_PATH="${TMP_DIR}/baseline_profile.json"
BASELINE_REPORT_PATH="${TMP_DIR}/baseline_report.json"

log "clear precompile profile"
run_update_call "clear_precompile_profile" "()"

run_workload "${WORKLOAD_RUNS}"
query_profile_json > "${BASELINE_JSON_PATH}"

python - "${BASELINE_JSON_PATH}" "${MIN_CALLS}" "${TARGET_GAS_PER_INSTRUCTION}" "${REFERENCE_PRECOMPILE_ADDRESS}" "${REFERENCE_TARGET_GAS}" "${SAFETY_MULTIPLIER}" "${MAX_DENOMINATOR}" "${BASELINE_REPORT_PATH}" <<'PY'
import json
import math
import sys
from fractions import Fraction

profile_path, min_calls_s, target_gpi_s, ref_addr_raw, ref_target_gas_s, safety_s, max_den_s, report_path = sys.argv[1:9]
min_calls = int(min_calls_s)
target_gpi_raw = target_gpi_s.strip()
ref_addr_raw = ref_addr_raw.strip().lower()
ref_target_gas = int(ref_target_gas_s) if ref_target_gas_s.strip() else None
safety = Fraction(safety_s)
max_den = int(max_den_s)

with open(profile_path, "r", encoding="utf-8") as f:
    raw = json.load(f)

def as_int(value):
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, int):
        return value
    if isinstance(value, str):
        return int(value, 10)
    raise ValueError(f"unsupported integer value: {value!r}")

def normalize_address(value):
    if isinstance(value, list):
        return "0x" + bytes(as_int(v) for v in value).hex()
    if isinstance(value, str):
        text = value.lower()
        if text.startswith("0x"):
            return text
        hex_chars = set("0123456789abcdef")
        if len(text) % 2 == 0 and set(text) <= hex_chars:
            return "0x" + text
        return text
    raise ValueError(f"unsupported address value: {value!r}")

entries = []
for item in raw:
    calls = as_int(item["calls"])
    total_instructions = as_int(item["total_instructions"])
    total_extra_gas = as_int(item["total_extra_gas"])
    avg_instructions = total_instructions // calls if calls else 0
    avg_extra_gas = total_extra_gas // calls if calls else 0
    entries.append(
        {
            "address": normalize_address(item["address"]),
            "calls": calls,
            "total_instructions": total_instructions,
            "avg_instructions": avg_instructions,
            "max_instructions": as_int(item["max_instructions"]),
            "total_extra_gas": total_extra_gas,
            "avg_extra_gas": avg_extra_gas,
            "max_extra_gas": as_int(item["max_extra_gas"]),
            "qualifies": calls >= min_calls,
        }
    )

entries.sort(key=lambda item: (-item["avg_instructions"], item["address"]))
qualifying = [item for item in entries if item["qualifies"]]

measurement = {
    "entry_count": len(entries),
    "qualifying_count": len(qualifying),
    "entries": entries,
}

recommendation = {
    "mode": "none",
    "target_gas_per_instruction": None,
    "numerator": None,
    "denominator": None,
    "reference_address": None,
    "reference_avg_instructions": None,
    "notes": [],
}

if target_gpi_raw:
    target_fraction = Fraction(target_gpi_raw).limit_denominator(max_den)
    recommendation.update(
        {
            "mode": "target_gas_per_instruction",
            "target_gas_per_instruction": str(target_fraction),
        }
    )
elif ref_addr_raw and ref_target_gas is not None:
    ref = next((item for item in entries if item["address"] == ref_addr_raw), None)
    if ref is None or ref["avg_instructions"] == 0:
        recommendation["notes"].append("reference precompile not observed in profile")
    else:
        target_fraction = Fraction(ref_target_gas, ref["avg_instructions"]) * safety
        recommendation.update(
            {
                "mode": "reference_precompile",
                "target_gas_per_instruction": str(target_fraction),
                "reference_address": ref["address"],
                "reference_avg_instructions": ref["avg_instructions"],
            }
        )
else:
    recommendation["notes"].append("no target configured; rerun with TARGET_GAS_PER_INSTRUCTION or REFERENCE_PRECOMPILE_ADDRESS/REFERENCE_TARGET_GAS")

target_value = recommendation["target_gas_per_instruction"]
if target_value:
    ratio = Fraction(target_value).limit_denominator(max_den)
    if ratio.numerator > (2**32 - 1) or ratio.denominator > (2**32 - 1):
        raise SystemExit("suggested ratio exceeds nat32 bounds")
    recommendation["numerator"] = ratio.numerator
    recommendation["denominator"] = ratio.denominator
    for item in entries:
        item["expected_avg_extra_gas"] = math.ceil(item["avg_instructions"] * ratio.numerator / ratio.denominator)
else:
    for item in entries:
        item["expected_avg_extra_gas"] = None

report = {
    "measurement": measurement,
    "recommendation": recommendation,
}

with open(report_path, "w", encoding="utf-8") as f:
    json.dump(report, f, ensure_ascii=False, indent=2)
PY

log "baseline summary"
python - "${BASELINE_REPORT_PATH}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as f:
    report = json.load(f)

print("[measure-precompile-ratio] qualifying profile entries:")
for item in report["measurement"]["entries"]:
    mark = "*" if item["qualifies"] else "-"
    expected = item["expected_avg_extra_gas"]
    expected_text = str(expected) if expected is not None else "n/a"
    print(
        f"[measure-precompile-ratio] {mark} {item['address']} calls={item['calls']} "
        f"avg_instructions={item['avg_instructions']} max_instructions={item['max_instructions']} "
        f"avg_extra_gas={item['avg_extra_gas']} expected_avg_extra_gas={expected_text}"
    )

recommendation = report["recommendation"]
if recommendation["numerator"] is None:
    for note in recommendation["notes"]:
        print(f"[measure-precompile-ratio] note: {note}")
else:
    print(
        f"[measure-precompile-ratio] suggested ratio={recommendation['numerator']}/{recommendation['denominator']} "
        f"target_gas_per_instruction={recommendation['target_gas_per_instruction']} "
        f"mode={recommendation['mode']}"
    )
PY

if [[ -n "${REPORT_JSON_PATH}" ]]; then
  cp "${BASELINE_REPORT_PATH}" "${REPORT_JSON_PATH}"
  log "wrote report json: ${REPORT_JSON_PATH}"
fi
