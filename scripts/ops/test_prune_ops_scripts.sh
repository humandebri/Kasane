#!/usr/bin/env bash
# where: script-level tests for prune ops helpers
# what: verify guardrails and tuning behavior with mocked dfx calls
# why: keep operational scripts safe and regression-resistant
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

MOCK_DFX="${TMP_DIR}/dfx"
CALLS_LOG="${TMP_DIR}/calls.log"
STATE_FILE="${TMP_DIR}/state.json"

cat >"${MOCK_DFX}" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
echo "$*" >> "${MOCK_CALLS_LOG}"
if printf '%s' "$*" | grep -q "get_prune_status"; then
  printf '%s\n' "${MOCK_PRUNE_STATUS_JSON}"
  exit 0
fi
if printf '%s' "$*" | grep -q "get_ops_status"; then
  printf '%s\n' "${MOCK_OPS_STATUS_JSON}"
  exit 0
fi
printf '(variant { Ok = null })\n'
EOF
chmod +x "${MOCK_DFX}"

fail() {
  echo "[test-prune-ops-scripts] FAIL: $*" >&2
  exit 1
}

assert_grep() {
  local pattern="$1"
  local file="$2"
  if ! grep -q -- "${pattern}" "${file}"; then
    fail "pattern not found: ${pattern} in ${file}"
  fi
}

run_apply_reject_case() {
  set +e
  MOCK_CALLS_LOG="${CALLS_LOG}" \
  MOCK_PRUNE_STATUS_JSON='{"pruning_enabled":true,"need_prune":true}' \
  MOCK_OPS_STATUS_JSON='{"prune_error_count":0,"mining_error_count":0,"instruction_soft_limit":4000000000}' \
  DFX_BIN="${MOCK_DFX}" \
  CANISTER_NAME_OR_ID="abcd" \
  NETWORK="ic" \
  RETAIN_BLOCKS="167" \
  "${REPO_ROOT}/scripts/ops/apply_prune_policy.sh" >/dev/null 2>&1
  local rc=$?
  set -e
  if [[ "${rc}" -eq 0 ]]; then
    fail "apply_prune_policy should reject RETAIN_BLOCKS=167"
  fi
}

run_apply_success_case() {
  : > "${CALLS_LOG}"
  MOCK_CALLS_LOG="${CALLS_LOG}" \
  MOCK_PRUNE_STATUS_JSON='{"pruning_enabled":true,"need_prune":false,"estimated_kept_bytes":100,"high_water_bytes":200,"hard_emergency_bytes":300,"pruned_before_block":null}' \
  MOCK_OPS_STATUS_JSON='{"prune_error_count":1,"mining_error_count":2,"instruction_soft_limit":4000000000}' \
  DFX_BIN="${MOCK_DFX}" \
  CANISTER_NAME_OR_ID="abcd" \
  NETWORK="ic" \
  TARGET_BYTES="0" \
  RETAIN_DAYS="14" \
  RETAIN_BLOCKS="168" \
  MAX_OPS_PER_TICK="300" \
  "${REPO_ROOT}/scripts/ops/apply_prune_policy.sh" >/dev/null

  assert_grep "set_prune_policy" "${CALLS_LOG}"
  assert_grep "set_pruning_enabled (true)" "${CALLS_LOG}"
  assert_grep "call --query abcd get_prune_status --output json" "${CALLS_LOG}"
  assert_grep "call --query abcd get_ops_status --output json" "${CALLS_LOG}"
}

run_tune_increase_case() {
  : > "${CALLS_LOG}"
  cat > "${STATE_FILE}" <<'EOF'
{
  "last_max_ops_per_tick": 300,
  "need_prune_streak": 1,
  "last_need_prune": true,
  "last_prune_error_count": 5,
  "last_mining_error_count": 7
}
EOF
  MOCK_CALLS_LOG="${CALLS_LOG}" \
  MOCK_PRUNE_STATUS_JSON='{"need_prune":true}' \
  MOCK_OPS_STATUS_JSON='{"prune_error_count":5,"mining_error_count":7}' \
  DFX_BIN="${MOCK_DFX}" \
  STATE_FILE="${STATE_FILE}" \
  CANISTER_NAME_OR_ID="abcd" \
  NETWORK="ic" \
  TARGET_BYTES="0" \
  RETAIN_DAYS="14" \
  RETAIN_BLOCKS="168" \
  MAX_OPS_PER_TICK_CURRENT="300" \
  "${REPO_ROOT}/scripts/ops/tune_prune_max_ops.sh" >/dev/null

  assert_grep "max_ops_per_tick = 500:nat32" "${CALLS_LOG}"
}

run_tune_decrease_case() {
  : > "${CALLS_LOG}"
  cat > "${STATE_FILE}" <<'EOF'
{
  "last_max_ops_per_tick": 100,
  "need_prune_streak": 2,
  "last_need_prune": true,
  "last_prune_error_count": 5,
  "last_mining_error_count": 7
}
EOF
  MOCK_CALLS_LOG="${CALLS_LOG}" \
  MOCK_PRUNE_STATUS_JSON='{"need_prune":true}' \
  MOCK_OPS_STATUS_JSON='{"prune_error_count":6,"mining_error_count":7}' \
  DFX_BIN="${MOCK_DFX}" \
  STATE_FILE="${STATE_FILE}" \
  CANISTER_NAME_OR_ID="abcd" \
  NETWORK="ic" \
  TARGET_BYTES="0" \
  RETAIN_DAYS="14" \
  RETAIN_BLOCKS="168" \
  MAX_OPS_PER_TICK_CURRENT="100" \
  "${REPO_ROOT}/scripts/ops/tune_prune_max_ops.sh" >/dev/null

  assert_grep "max_ops_per_tick = 1:nat32" "${CALLS_LOG}"
}

run_apply_reject_case
run_apply_success_case
run_tune_increase_case
run_tune_decrease_case

echo "[test-prune-ops-scripts] ok"
