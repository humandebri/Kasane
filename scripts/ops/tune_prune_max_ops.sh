#!/usr/bin/env bash
# where: ops helper for prune max_ops tuning
# what: adjust max_ops_per_tick based on need_prune continuity and error counters
# why: keep 84-block prune cadence safe while avoiding instruction pressure spikes
# note: timer interval is fixed by canister-side event-driven scheduling
set -euo pipefail

CANISTER_NAME_OR_ID="${CANISTER_NAME_OR_ID:-}"
NETWORK="${NETWORK:-ic}"
TARGET_BYTES="${TARGET_BYTES:-0}"
RETAIN_DAYS="${RETAIN_DAYS:-14}"
RETAIN_BLOCKS="${RETAIN_BLOCKS:-168}"
MAX_OPS_PER_TICK_CURRENT="${MAX_OPS_PER_TICK_CURRENT:-}"
HEADROOM_RATIO_BPS="${HEADROOM_RATIO_BPS:-2000}"
HARD_EMERGENCY_RATIO_BPS="${HARD_EMERGENCY_RATIO_BPS:-9500}"
MAX_OPS_STEP="${MAX_OPS_STEP:-200}"
DFX_BIN="${DFX_BIN:-dfx}"
STATE_FILE="${STATE_FILE:-/tmp/kasane-prune-tune-state-${NETWORK}-${CANISTER_NAME_OR_ID}.json}"

log() {
  echo "[tune-prune-max-ops] $*"
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[tune-prune-max-ops] missing command: $1" >&2
    exit 1
  fi
}

require_non_negative_int() {
  local name="$1"
  local value="$2"
  if [[ ! "${value}" =~ ^[0-9]+$ ]]; then
    echo "[tune-prune-max-ops] ${name} must be a non-negative integer: ${value}" >&2
    exit 1
  fi
}

if [[ -z "${CANISTER_NAME_OR_ID}" ]]; then
  echo "[tune-prune-max-ops] missing CANISTER_NAME_OR_ID" >&2
  echo "usage: CANISTER_NAME_OR_ID=<id> NETWORK=ic TARGET_BYTES=0 RETAIN_DAYS=14 RETAIN_BLOCKS=168 MAX_OPS_PER_TICK_CURRENT=300 scripts/ops/tune_prune_max_ops.sh" >&2
  exit 1
fi

require_cmd "${DFX_BIN}"
require_non_negative_int "TARGET_BYTES" "${TARGET_BYTES}"
require_non_negative_int "RETAIN_DAYS" "${RETAIN_DAYS}"
require_non_negative_int "RETAIN_BLOCKS" "${RETAIN_BLOCKS}"
require_non_negative_int "HEADROOM_RATIO_BPS" "${HEADROOM_RATIO_BPS}"
require_non_negative_int "HARD_EMERGENCY_RATIO_BPS" "${HARD_EMERGENCY_RATIO_BPS}"
require_non_negative_int "MAX_OPS_STEP" "${MAX_OPS_STEP}"

if (( RETAIN_BLOCKS < 168 )); then
  echo "[tune-prune-max-ops] RETAIN_BLOCKS must be >= 168 under 84-block prune cadence" >&2
  exit 1
fi
if (( MAX_OPS_STEP < 1 )); then
  echo "[tune-prune-max-ops] MAX_OPS_STEP must be >= 1" >&2
  exit 1
fi

PRUNE_STATUS_JSON="$("${DFX_BIN}" canister --network "${NETWORK}" call --query "${CANISTER_NAME_OR_ID}" get_prune_status --output json)"
OPS_STATUS_JSON="$("${DFX_BIN}" canister --network "${NETWORK}" call --query "${CANISTER_NAME_OR_ID}" get_ops_status --output json)"

if [[ -z "${MAX_OPS_PER_TICK_CURRENT}" ]]; then
  if [[ -f "${STATE_FILE}" ]]; then
    MAX_OPS_PER_TICK_CURRENT="$(python - <<PY
import json
with open("${STATE_FILE}", "r", encoding="utf-8") as f:
    data = json.load(f)
print(int(data.get("last_max_ops_per_tick", 0)))
PY
)"
  fi
fi

if [[ -z "${MAX_OPS_PER_TICK_CURRENT}" || ! "${MAX_OPS_PER_TICK_CURRENT}" =~ ^[0-9]+$ ]]; then
  echo "[tune-prune-max-ops] MAX_OPS_PER_TICK_CURRENT is required (or state file must provide it)" >&2
  exit 1
fi

if (( MAX_OPS_PER_TICK_CURRENT < 1 )); then
  echo "[tune-prune-max-ops] MAX_OPS_PER_TICK_CURRENT must be >= 1" >&2
  exit 1
fi

mkdir -p "$(dirname "${STATE_FILE}")"

TUNED_LINE="$(python - <<PY
import json
from pathlib import Path

prune = json.loads("""${PRUNE_STATUS_JSON}""")
ops = json.loads("""${OPS_STATUS_JSON}""")
state_path = Path("${STATE_FILE}")
current_max_ops = int("${MAX_OPS_PER_TICK_CURRENT}")
step = int("${MAX_OPS_STEP}")

prev = {}
if state_path.exists():
    prev = json.loads(state_path.read_text(encoding="utf-8"))

prev_need_streak = int(prev.get("need_prune_streak", 0))
prev_prune_err = int(prev.get("last_prune_error_count", 0))
prev_mining_err = int(prev.get("last_mining_error_count", 0))

need_prune_now = bool(prune.get("need_prune", False))
prune_err_now = int(ops.get("prune_error_count", 0))
mining_err_now = int(ops.get("mining_error_count", 0))

errors_increased = prune_err_now > prev_prune_err or mining_err_now > prev_mining_err
need_prune_streak = (prev_need_streak + 1) if need_prune_now else 0

new_max_ops = current_max_ops
reason = "hold"
if errors_increased:
    new_max_ops = max(1, current_max_ops - step)
    reason = "decrease_on_error_growth"
elif need_prune_streak >= 2:
    new_max_ops = current_max_ops + step
    reason = "increase_on_continuous_need_prune"

state = {
    "last_max_ops_per_tick": int(new_max_ops),
    "need_prune_streak": int(need_prune_streak),
    "last_need_prune": bool(need_prune_now),
    "last_prune_error_count": int(prune_err_now),
    "last_mining_error_count": int(mining_err_now),
}
state_path.write_text(json.dumps(state, ensure_ascii=False, indent=2), encoding="utf-8")

print(
    "|".join(
        [
            str(new_max_ops),
            reason,
            str(need_prune_streak),
            "1" if errors_increased else "0",
            str(prune_err_now),
            str(mining_err_now),
        ]
    )
)
PY
)"

IFS='|' read -r NEW_MAX_OPS REASON NEED_PRUNE_STREAK ERRORS_INCREASED PRUNE_ERR_NOW MINING_ERR_NOW <<< "${TUNED_LINE}"

POLICY_ARG="(record {
  headroom_ratio_bps = ${HEADROOM_RATIO_BPS}:nat32;
  target_bytes = ${TARGET_BYTES}:nat64;
  retain_blocks = ${RETAIN_BLOCKS}:nat64;
  retain_days = ${RETAIN_DAYS}:nat64;
  hard_emergency_ratio_bps = ${HARD_EMERGENCY_RATIO_BPS}:nat32;
  max_ops_per_tick = ${NEW_MAX_OPS}:nat32;
})"

log "reason=${REASON} current_max_ops=${MAX_OPS_PER_TICK_CURRENT} new_max_ops=${NEW_MAX_OPS} need_prune_streak=${NEED_PRUNE_STREAK} errors_increased=${ERRORS_INCREASED}"
"${DFX_BIN}" canister --network "${NETWORK}" call "${CANISTER_NAME_OR_ID}" set_prune_policy "${POLICY_ARG}" >/dev/null

log "set_prune_policy applied; counters prune_error_count=${PRUNE_ERR_NOW} mining_error_count=${MINING_ERR_NOW}"
log "state_file=${STATE_FILE}"
