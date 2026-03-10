#!/usr/bin/env bash
# where: local/CI guard
# what: ensure generated DID matches the tracked file
# why: prevent interface drift between Rust API and published DID

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
EXPECTED_DID="${REPO_ROOT}/crates/ic-evm-gateway/evm_canister.did"
GENERATED_DID="$(mktemp -t evm_canister.generated.XXXXXX.did)"
NORMALIZED_EXPECTED="$(mktemp -t evm_canister.expected.XXXXXX.did)"
NORMALIZED_GENERATED="$(mktemp -t evm_canister.generated.normalized.XXXXXX.did)"

cleanup() {
  rm -f "${GENERATED_DID}"
  rm -f "${NORMALIZED_EXPECTED}"
  rm -f "${NORMALIZED_GENERATED}"
}
trap cleanup EXIT

cargo run -q -p ic-evm-gateway --features did-gen --bin export_did > "${GENERATED_DID}"

grep -Ev '^[[:space:]]*//' "${EXPECTED_DID}" > "${NORMALIZED_EXPECTED}"
grep -Ev '^[[:space:]]*//' "${GENERATED_DID}" > "${NORMALIZED_GENERATED}"

if ! diff -u "${NORMALIZED_EXPECTED}" "${NORMALIZED_GENERATED}"; then
  echo "[guard] DID mismatch detected. Regenerate and sync evm_canister.did." >&2
  exit 1
fi

echo "[guard] DID sync ok"
