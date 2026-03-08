#!/usr/bin/env bash
# where: script-level tests for init arg helpers
# what: verify wrap canister id must be explicit when building init args
# why: remove hidden lookup paths and keep deploy preconditions obvious
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

MOCK_BIN_DIR="${TMP_DIR}/bin"
mkdir -p "${MOCK_BIN_DIR}"

ICP_LOG="${TMP_DIR}/icp.log"
cat > "${MOCK_BIN_DIR}/icp" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
echo "$*" >> "${MOCK_ICP_LOG}"
if [[ "${MOCK_ICP_STDOUT:-}" != "" ]]; then
  printf '%s\n' "${MOCK_ICP_STDOUT}"
fi
exit "${MOCK_ICP_EXIT_CODE:-0}"
EOF
chmod +x "${MOCK_BIN_DIR}/icp"

fail() {
  echo "[test-lib-init-args] FAIL: $*" >&2
  exit 1
}

assert_eq() {
  local actual="$1"
  local expected="$2"
  if [[ "${actual}" != "${expected}" ]]; then
    fail "expected '${expected}', got '${actual}'"
  fi
}

run_resolve() {
  env \
    PATH="${MOCK_BIN_DIR}:${PATH}" \
    MOCK_ICP_LOG="${ICP_LOG}" \
    "$@" \
    bash -lc 'source "'"${REPO_ROOT}"'/scripts/lib_init_args.sh"; resolve_wrap_canister_id'
}

assert_file_empty() {
  local file="$1"
  if [[ -s "${file}" ]]; then
    fail "expected empty file: ${file}"
  fi
}

run_env_precedence_case() {
  : > "${ICP_LOG}"
  local out
  out="$(run_resolve WRAP_CANISTER_ID="env-wrap-id" MOCK_ICP_STDOUT="unexpected-id" MOCK_ICP_EXIT_CODE=0)"
  assert_eq "${out}" "env-wrap-id"
  assert_file_empty "${ICP_LOG}"
}

run_failure_case() {
  : > "${ICP_LOG}"
  set +e
  local out
  out="$(run_resolve MOCK_ICP_STDOUT="unexpected-id" MOCK_ICP_EXIT_CODE=0 2>&1)"
  local rc=$?
  set -e
  if [[ "${rc}" -eq 0 ]]; then
    fail "resolve_wrap_canister_id should fail without WRAP_CANISTER_ID"
  fi
  if [[ "${out}" != *"WRAP_CANISTER_ID is required"* ]]; then
    fail "expected error output, got: ${out}"
  fi
  assert_file_empty "${ICP_LOG}"
}

run_env_precedence_case
run_failure_case

echo "[test-lib-init-args] ok"
