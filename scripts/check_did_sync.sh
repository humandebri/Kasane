#!/usr/bin/env bash
# where: local/CI guard
# what: ensure generated DID matches the tracked file
# why: prevent interface drift between Rust API and published DID

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEFAULT_DID="${REPO_ROOT}/crates/ic-evm-gateway/evm_canister.did"
ADMIN_DID="${REPO_ROOT}/crates/ic-evm-gateway/evm_canister_precompile_profile_admin.did"

check_did_sync() {
  local label="$1"
  local expected_did="$2"
  local features="$3"
  local generated_did
  local normalized_expected
  local normalized_generated

  generated_did="$(mktemp -t evm_canister.generated.XXXXXX.did)"
  normalized_expected="$(mktemp -t evm_canister.expected.XXXXXX.did)"
  normalized_generated="$(mktemp -t evm_canister.generated.normalized.XXXXXX.did)"

  cargo run -q -p ic-evm-gateway --features "${features}" --bin export_did > "${generated_did}"

  grep -Ev '^[[:space:]]*//' "${expected_did}" > "${normalized_expected}"
  grep -Ev '^[[:space:]]*//' "${generated_did}" > "${normalized_generated}"

  if ! diff -u "${normalized_expected}" "${normalized_generated}"; then
    rm -f "${generated_did}" "${normalized_expected}" "${normalized_generated}"
    echo "[guard] DID mismatch detected for ${label}: ${expected_did}" >&2
    return 1
  fi

  rm -f "${generated_did}" "${normalized_expected}" "${normalized_generated}"
  echo "[guard] DID sync ok: ${label}"
}

check_did_sync "default" "${DEFAULT_DID}" "did-gen"
check_did_sync "precompile-profile-admin" "${ADMIN_DID}" "did-gen precompile-profile-admin"
