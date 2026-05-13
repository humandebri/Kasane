#!/usr/bin/env bash
# where: script-level tests for legacy wrap drain gate
# what: verify preflight helper accepts only drained legacy requests
# why: mainnet upgrade must not strand old standalone wrap canister requests
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${REPO_ROOT}/scripts/lib_legacy_wrap_drain.sh"

fail() {
  echo "[test-legacy-wrap-drain] FAIL: $*" >&2
  exit 1
}

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

FAKE_DFX="${TMP_DIR}/dfx"
cat > "${FAKE_DFX}" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
echo "$*" >> "${FAKE_DFX_CALLS}"
method=""
for arg in "$@"; do
  if [[ "${arg}" == "get_request" || "${arg}" == "get_native_deposit_result" ]]; then
    method="${arg}"
  fi
done
case "${FAKE_DFX_MODE}:${method}" in
  terminal:get_request)
    echo '(opt record { status = variant { Succeeded }; dispatch_status = null })'
    ;;
  failed_terminal:get_request)
    echo '(opt record { status = variant { Failed }; dispatch_status = null })'
    ;;
  active:get_request)
    echo '(opt record { status = variant { Running }; dispatch_status = null })'
    ;;
  dispatch_failed:get_request)
    echo '(opt record { status = variant { Failed }; dispatch_status = opt variant { DispatchFailed } })'
    ;;
  native_terminal:get_request)
    echo '(null)'
    ;;
  native_terminal:get_native_deposit_result)
    echo '(opt record { status = variant { Succeeded }; dispatch_status = null })'
    ;;
  missing:get_request|missing:get_native_deposit_result)
    echo '(null)'
    ;;
  *)
    echo "(null)"
    ;;
esac
SH
chmod +x "${FAKE_DFX}"

export DFX_BIN="${FAKE_DFX}"
export FAKE_DFX_CALLS="${TMP_DIR}/calls.log"
export ICP_ENV="ic"
export ICP_IDENTITY_NAME="ci-local"
export LEGACY_WRAP_CANISTER_ID="lpuz5-uyaaa-aaaam-ah4da-cai"
export EVM_CANISTER_ID="4c52m-aiaaa-aaaam-agwwa-cai"
export LEGACY_WRAP_CANISTER_DID="crates/ic-evm-gateway/evm_canister.did"

REQUESTS_FILE="${TMP_DIR}/requests.txt"
export LEGACY_WRAP_REQUEST_IDS_FILE="${REQUESTS_FILE}"
REQUEST_ID="0x1111111111111111111111111111111111111111111111111111111111111111"

expect_ok() {
  local label="$1"
  if ! check_legacy_wrap_drain >/tmp/legacy-wrap-drain-ok.out 2>/tmp/legacy-wrap-drain-ok.err; then
    cat /tmp/legacy-wrap-drain-ok.out >&2
    cat /tmp/legacy-wrap-drain-ok.err >&2
    fail "${label}"
  fi
}

expect_fail() {
  local label="$1"
  if check_legacy_wrap_drain >/tmp/legacy-wrap-drain-fail.out 2>/tmp/legacy-wrap-drain-fail.err; then
    cat /tmp/legacy-wrap-drain-fail.out >&2
    fail "${label}"
  fi
}

: > "${REQUESTS_FILE}"
unset ALLOW_EMPTY_LEGACY_WRAP_REQUESTS || true
expect_fail "empty manifest without attestation should fail"
export ALLOW_EMPTY_LEGACY_WRAP_REQUESTS="1"
expect_ok "empty manifest with attestation should pass"
unset ALLOW_EMPTY_LEGACY_WRAP_REQUESTS

printf '%s\n' "${REQUEST_ID}" > "${REQUESTS_FILE}"
FAKE_DFX_MODE="terminal"; export FAKE_DFX_MODE; expect_ok "succeeded request should pass"
FAKE_DFX_MODE="failed_terminal"; export FAKE_DFX_MODE; expect_ok "terminal failed request should pass"
FAKE_DFX_MODE="native_terminal"; export FAKE_DFX_MODE; expect_ok "native terminal request should pass"
FAKE_DFX_MODE="active"; export FAKE_DFX_MODE; expect_fail "active request should fail"
FAKE_DFX_MODE="dispatch_failed"; export FAKE_DFX_MODE; expect_fail "dispatch failed request should fail"
FAKE_DFX_MODE="missing"; export FAKE_DFX_MODE; expect_fail "missing request should fail"

grep -q -- "--query" "${FAKE_DFX_CALLS}" || fail "dfx calls must use --query"

echo "[test-legacy-wrap-drain] ok"
