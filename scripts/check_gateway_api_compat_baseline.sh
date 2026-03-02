#!/usr/bin/env bash
# where: local/CI guard
# what: verify gateway API compatibility baseline remains backward-compatible
# why: prevent canister API changes from breaking gateway consumers

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BASELINE_DID="${REPO_ROOT}/tools/rpc-gateway/contracts/gateway-api-compat-baseline.did"
BASELINE_METHODS="${REPO_ROOT}/tools/rpc-gateway/contracts/gateway-api-compat-methods.txt"
GENERATED_DID="$(mktemp -t gateway_api_compat.generated.XXXXXX.did)"
EXTRACTED_BASELINE="$(mktemp -t gateway_api_compat.baseline.extracted.XXXXXX.did)"
EXTRACTED_DID="$(mktemp -t gateway_api_compat.extracted.XXXXXX.did)"
NORMALIZED_EXPECTED="$(mktemp -t gateway_api_compat.expected.XXXXXX.did)"
NORMALIZED_GENERATED="$(mktemp -t gateway_api_compat.generated.normalized.XXXXXX.did)"

cleanup() {
  rm -f "${GENERATED_DID}" "${EXTRACTED_BASELINE}" "${EXTRACTED_DID}" "${NORMALIZED_EXPECTED}" "${NORMALIZED_GENERATED}"
}
trap cleanup EXIT

extract_minimal_service() {
  local input_did="$1"
  local output_did="$2"
  awk -v methods_file="${BASELINE_METHODS}" '
    BEGIN {
      while ((getline line < methods_file) > 0) {
        required[line] = 1
      }
      in_service = 0
      in_method = 0
      keep_method = 0
    }

    {
      if (!in_service) {
        if ($0 ~ /^service[[:space:]]*:/) {
          print $0
          in_service = 1
        }
        next
      }

      if (!in_method) {
        if ($0 ~ /^[[:space:]]*}[[:space:]]*;?[[:space:]]*$/) {
          print $0
          in_service = 0
          next
        }

        method_line = $0
        method_name = ""
        if (method_line ~ /^[[:space:]]*[[:alnum:]_]+[[:space:]]*:/) {
          method_name = method_line
          sub(/^[[:space:]]*/, "", method_name)
          sub(/[[:space:]]*:.*$/, "", method_name)
        }

        keep_method = (method_name in required)
        in_method = 1

        if (keep_method) {
          print $0
        }

        if (index($0, ";") > 0) {
          in_method = 0
          keep_method = 0
        }
        next
      }

      if (keep_method) {
        print $0
      }

      if (index($0, ";") > 0) {
        in_method = 0
        keep_method = 0
      }
    }
  ' "${input_did}" > "${output_did}"
}

validate_methods_file() {
  if [[ ! -f "${BASELINE_METHODS}" ]]; then
    echo "[guard] methods file not found: ${BASELINE_METHODS}" >&2
    exit 1
  fi
  if ! awk 'NF { seen[$0] += 1 } END { for (k in seen) if (seen[k] > 1) exit 1 }' "${BASELINE_METHODS}"; then
    echo "[guard] duplicate method found in methods file: ${BASELINE_METHODS}" >&2
    exit 1
  fi
}

validate_extracted_methods() {
  local extracted_did="$1"
  while IFS= read -r method; do
    [[ -z "${method}" ]] && continue
    local count
    count="$(grep -Ec "^[[:space:]]*${method}[[:space:]]*:" "${extracted_did}" || true)"
    if [[ "${count}" != "1" ]]; then
      echo "[guard] extracted method mismatch: ${method} (count=${count})" >&2
      exit 1
    fi
  done < "${BASELINE_METHODS}"
}

validate_methods_file

if [[ "${1:-}" == "--update" ]]; then
  cargo run -q -p ic-evm-wrapper --features did-gen --bin export_did > "${GENERATED_DID}"
  extract_minimal_service "${GENERATED_DID}" "${BASELINE_DID}"
  validate_extracted_methods "${BASELINE_DID}"
  echo "[guard] updated baseline: ${BASELINE_DID}"
  exit 0
fi

if [[ ! -f "${BASELINE_DID}" ]]; then
  echo "[guard] baseline did not found: ${BASELINE_DID}" >&2
  echo "[guard] bootstrap with: scripts/check_gateway_api_compat_baseline.sh --update" >&2
  exit 1
fi

cargo run -q -p ic-evm-wrapper --features did-gen --bin export_did > "${GENERATED_DID}"
extract_minimal_service "${BASELINE_DID}" "${EXTRACTED_BASELINE}"
extract_minimal_service "${GENERATED_DID}" "${EXTRACTED_DID}"
validate_extracted_methods "${EXTRACTED_BASELINE}"
validate_extracted_methods "${EXTRACTED_DID}"

grep -Ev '^[[:space:]]*//' "${EXTRACTED_BASELINE}" > "${NORMALIZED_EXPECTED}"
grep -Ev '^[[:space:]]*//' "${EXTRACTED_DID}" > "${NORMALIZED_GENERATED}"

if ! diff -u "${NORMALIZED_EXPECTED}" "${NORMALIZED_GENERATED}"; then
  echo "[guard] gateway API compatibility baseline mismatch detected." >&2
  echo "[guard] allowed: adding non-baseline methods and non-baseline type definitions" >&2
  echo "[guard] forbidden: deleting/changing baseline method signatures or query/update attrs" >&2
  echo "[guard] if intentional baseline bump: update docs + baseline files + matrix in same PR" >&2
  exit 1
fi

echo "[guard] gateway API compatibility baseline ok"
