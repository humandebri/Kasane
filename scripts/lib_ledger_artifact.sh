#!/usr/bin/env bash
# where: local CI/smoke shared helper
# what: vendored ledger wasm と release 別 DID cache の path/取得を共通化する
# why: local smoke と CI helper の release/path 規則をずらさないため

set -euo pipefail

ledger_artifact_resolve_release() {
  local release_tag="$1"
  if [[ "${release_tag}" == "latest" ]]; then
    echo "[ledger-artifact] LEDGER_RELEASE=latest is not supported" >&2
    echo "[ledger-artifact] set a fixed release tag or vendor third_party/dfinity/<release>/ic-icrc1-ledger.wasm first" >&2
    return 1
  fi
  echo "${release_tag}"
}

ledger_artifact_wasm_path() {
  local root_dir="$1"
  local release_tag="$2"
  echo "${root_dir}/third_party/dfinity/${release_tag}/ic-icrc1-ledger.wasm"
}

ledger_artifact_did_dir() {
  local cache_dir="$1"
  local release_tag="$2"
  echo "${cache_dir}/${release_tag}"
}

ledger_artifact_did_path() {
  local cache_dir="$1"
  local release_tag="$2"
  echo "$(ledger_artifact_did_dir "${cache_dir}" "${release_tag}")/ledger.did"
}

ledger_artifact_require_curl() {
  if ! command -v curl >/dev/null 2>&1; then
    echo "[ledger-artifact] missing command: curl" >&2
    return 1
  fi
}

ledger_artifact_ensure_did() {
  local cache_dir="$1"
  local release_tag="$2"
  local did_dir
  local did_path
  ledger_artifact_require_curl
  did_dir="$(ledger_artifact_did_dir "${cache_dir}" "${release_tag}")"
  did_path="$(ledger_artifact_did_path "${cache_dir}" "${release_tag}")"
  mkdir -p "${did_dir}"
  if [[ -f "${did_path}" ]]; then
    return 0
  fi
  curl -L --fail --output "${did_path}" "https://github.com/dfinity/ic/releases/download/${release_tag}/ledger.did"
}
