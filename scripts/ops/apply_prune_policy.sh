#!/usr/bin/env bash
# where: ops helper for canister prune settings
# what: apply prune policy + enable pruning + print verification summary
# why: reduce operator error under 84-block prune execution model
set -euo pipefail

CANISTER_NAME_OR_ID="${CANISTER_NAME_OR_ID:-}"
NETWORK="${NETWORK:-ic}"
TARGET_BYTES="${TARGET_BYTES:-0}"
RETAIN_DAYS="${RETAIN_DAYS:-14}"
RETAIN_BLOCKS="${RETAIN_BLOCKS:-168}"
MAX_OPS_PER_TICK="${MAX_OPS_PER_TICK:-300}"
HEADROOM_RATIO_BPS="${HEADROOM_RATIO_BPS:-2000}"
HARD_EMERGENCY_RATIO_BPS="${HARD_EMERGENCY_RATIO_BPS:-9500}"
DFX_BIN="${DFX_BIN:-dfx}"

log() {
  echo "[apply-prune-policy] $*"
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[apply-prune-policy] missing command: $1" >&2
    exit 1
  fi
}

require_non_negative_int() {
  local name="$1"
  local value="$2"
  if [[ ! "${value}" =~ ^[0-9]+$ ]]; then
    echo "[apply-prune-policy] ${name} must be a non-negative integer: ${value}" >&2
    exit 1
  fi
}

if [[ -z "${CANISTER_NAME_OR_ID}" ]]; then
  echo "[apply-prune-policy] missing CANISTER_NAME_OR_ID" >&2
  echo "usage: CANISTER_NAME_OR_ID=<id> NETWORK=ic TARGET_BYTES=0 RETAIN_DAYS=14 RETAIN_BLOCKS=168 MAX_OPS_PER_TICK=300 scripts/ops/apply_prune_policy.sh" >&2
  exit 1
fi

require_cmd "${DFX_BIN}"
require_non_negative_int "TARGET_BYTES" "${TARGET_BYTES}"
require_non_negative_int "RETAIN_DAYS" "${RETAIN_DAYS}"
require_non_negative_int "RETAIN_BLOCKS" "${RETAIN_BLOCKS}"
require_non_negative_int "MAX_OPS_PER_TICK" "${MAX_OPS_PER_TICK}"
require_non_negative_int "HEADROOM_RATIO_BPS" "${HEADROOM_RATIO_BPS}"
require_non_negative_int "HARD_EMERGENCY_RATIO_BPS" "${HARD_EMERGENCY_RATIO_BPS}"

if (( RETAIN_BLOCKS < 168 )); then
  echo "[apply-prune-policy] RETAIN_BLOCKS must be >= 168 under 84-block prune cadence" >&2
  exit 1
fi
if (( MAX_OPS_PER_TICK < 1 )); then
  echo "[apply-prune-policy] MAX_OPS_PER_TICK must be >= 1" >&2
  exit 1
fi

POLICY_ARG="(record {
  headroom_ratio_bps = ${HEADROOM_RATIO_BPS}:nat32;
  target_bytes = ${TARGET_BYTES}:nat64;
  retain_blocks = ${RETAIN_BLOCKS}:nat64;
  retain_days = ${RETAIN_DAYS}:nat64;
  hard_emergency_ratio_bps = ${HARD_EMERGENCY_RATIO_BPS}:nat32;
  max_ops_per_tick = ${MAX_OPS_PER_TICK}:nat32;
})"

log "set_prune_policy: canister=${CANISTER_NAME_OR_ID} network=${NETWORK}"
"${DFX_BIN}" canister --network "${NETWORK}" call "${CANISTER_NAME_OR_ID}" set_prune_policy "${POLICY_ARG}" >/dev/null
log "set_pruning_enabled(true): canister=${CANISTER_NAME_OR_ID} network=${NETWORK}"
"${DFX_BIN}" canister --network "${NETWORK}" call "${CANISTER_NAME_OR_ID}" set_pruning_enabled "(true)" >/dev/null

PRUNE_STATUS_JSON="$("${DFX_BIN}" canister --network "${NETWORK}" call --query "${CANISTER_NAME_OR_ID}" get_prune_status --output json)"
OPS_STATUS_JSON="$("${DFX_BIN}" canister --network "${NETWORK}" call --query "${CANISTER_NAME_OR_ID}" get_ops_status --output json)"

python - <<PY
import json
prune = json.loads("""${PRUNE_STATUS_JSON}""")
ops = json.loads("""${OPS_STATUS_JSON}""")
summary = {
  "pruning_enabled": prune.get("pruning_enabled"),
  "need_prune": prune.get("need_prune"),
  "estimated_kept_bytes": prune.get("estimated_kept_bytes"),
  "high_water_bytes": prune.get("high_water_bytes"),
  "hard_emergency_bytes": prune.get("hard_emergency_bytes"),
  "pruned_before_block": prune.get("pruned_before_block"),
  "prune_error_count": ops.get("prune_error_count"),
  "mining_error_count": ops.get("mining_error_count"),
  "instruction_soft_limit": ops.get("instruction_soft_limit"),
}
print("[apply-prune-policy] verification:", json.dumps(summary, ensure_ascii=False))
PY

echo "[apply-prune-policy] restore command:"
echo "${DFX_BIN} canister --network ${NETWORK} call ${CANISTER_NAME_OR_ID} set_pruning_enabled '(false)'"
