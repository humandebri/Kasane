#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: $0 <baseline.yml> <current.yml>" >&2
  exit 2
fi

BASELINE_FILE="$1"
CURRENT_FILE="$2"

if [[ ! -f "$BASELINE_FILE" ]]; then
  echo "[canbench-guard] baseline file not found: $BASELINE_FILE" >&2
  exit 2
fi
if [[ ! -f "$CURRENT_FILE" ]]; then
  echo "[canbench-guard] current file not found: $CURRENT_FILE" >&2
  exit 2
fi

MAX_REGRESSION_PCT="${CANBENCH_MAX_REGRESSION_PCT:-2.0}"
TARGET_IMPROVEMENT_PCT="${CANBENCH_TARGET_IMPROVEMENT_PCT:-5.0}"
TARGET_BENCHES_CSV="${CANBENCH_TARGET_BENCHES:-}"

echo "[canbench-guard] thresholds: non-target regression <= +${MAX_REGRESSION_PCT}% , target improvement >= ${TARGET_IMPROVEMENT_PCT}%"

awk \
  -v baseline_file="$BASELINE_FILE" \
  -v current_file="$CURRENT_FILE" \
  -v max_regression="$MAX_REGRESSION_PCT" \
  -v target_improvement="$TARGET_IMPROVEMENT_PCT" \
  -v target_csv="$TARGET_BENCHES_CSV" '
function parse_file(path, dest,    line, bench, in_total, parts, value) {
  bench = ""
  in_total = 0
  while ((getline line < path) > 0) {
    if (line ~ /^  [^ ].*:[[:space:]]*$/) {
      bench = line
      sub(/^  /, "", bench)
      sub(/:[[:space:]]*$/, "", bench)
      in_total = 0
      continue
    }
    if (line ~ /^    total:[[:space:]]*$/) {
      in_total = 1
      continue
    }
    if (line ~ /^    scopes:[[:space:]]*$/) {
      in_total = 0
      continue
    }
    if (in_total && line ~ /^      instructions:[[:space:]]*[0-9]+[[:space:]]*$/) {
      split(line, parts, /:[[:space:]]*/)
      value = parts[2]
      gsub(/[^0-9]/, "", value)
      if (bench != "" && value != "") {
        dest[bench] = value + 0
      }
    }
  }
  close(path)
}

function is_target(name,    csv, token_count, tokens, i) {
  if (target_csv == "") {
    return 0
  }
  csv = target_csv
  gsub(/[[:space:]]+/, "", csv)
  token_count = split(csv, tokens, ",")
  for (i = 1; i <= token_count; i++) {
    if (tokens[i] == name) {
      return 1
    }
  }
  return 0
}

BEGIN {
  fail = 0
  parse_file(baseline_file, base)
  parse_file(current_file, cur)

  for (name in base) {
    if (!(name in cur)) {
      printf("[canbench-guard] ERROR: benchmark missing in current results: %s\n", name)
      fail = 1
    }
  }

  for (name in cur) {
    cur_v = cur[name]
    if (!(name in base)) {
      printf("[canbench-guard] INFO: new benchmark (no baseline): %s = %d\n", name, cur_v)
      continue
    }

    base_v = base[name]
    pct = ((cur_v - base_v) * 100.0) / base_v

    if (is_target(name)) {
      improvement = -pct
      if (improvement + 1e-12 < target_improvement) {
        printf("[canbench-guard] FAIL(target): %s base=%d current=%d delta=%.2f%% (improvement %.2f%% < %.2f%%)\n", name, base_v, cur_v, pct, improvement, target_improvement + 0.0)
        fail = 1
      } else {
        printf("[canbench-guard] PASS(target): %s base=%d current=%d delta=%.2f%%\n", name, base_v, cur_v, pct)
      }
    } else {
      if (pct - 1e-12 > max_regression) {
        printf("[canbench-guard] FAIL(non-target): %s base=%d current=%d delta=+%.2f%% (> +%.2f%%)\n", name, base_v, cur_v, pct, max_regression + 0.0)
        fail = 1
      } else {
        printf("[canbench-guard] PASS(non-target): %s base=%d current=%d delta=%.2f%%\n", name, base_v, cur_v, pct)
      }
    }
  }

  if (fail != 0) {
    print "[canbench-guard] RESULT: FAILED"
    exit 1
  }

  print "[canbench-guard] RESULT: PASSED"
}
'
