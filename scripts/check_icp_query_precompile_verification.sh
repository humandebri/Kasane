#!/usr/bin/env bash
# where: PR #81 ICP query precompile verification gate
# what: run the proof/test set and check specgen artifacts for the PR-local target set
# why: specgen gate currently requires every changed Rust function, including async adapters and tests
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
cd "${repo_root}"

log() {
  echo "[icp-query-precompile-verification] $*"
}

fail() {
  echo "[icp-query-precompile-verification] $*" >&2
  exit 1
}

require_file() {
  local path="$1"
  [[ -f "${path}" ]] || fail "missing required file: ${path}"
}

require_contains() {
  local path="$1"
  local pattern="$2"
  if ! grep -Fq -- "${pattern}" "${path}"; then
    fail "missing pattern in ${path}: ${pattern}"
  fi
}

require_specgen_status_pass() {
  local target="$1"
  specgen status "${target}" --check >/dev/null
}

target_file_for_slug() {
  case "$1" in
    compact_icp_query_input_safe_raw-8605da94)
      echo "crates/verified-core/src/kasane_precompiles/compact_icp_query_input.rs"
      ;;
    icp_query_update_kind_rejected_raw-b2b79d8e)
      echo "crates/verified-core/src/kasane_precompiles/icp_query_update_kind_rejected.rs"
      ;;
    icp_query_gas_observation_safe_raw-9b7ab62f)
      echo "crates/verified-core/src/kasane_precompiles/icp_query_gas_observation.rs"
      ;;
    icp_precompile_allowlist_entry_safe_raw-0ba30703)
      echo "crates/verified-core/src/kasane_precompiles/icp_precompile_allowlist_entry.rs"
      ;;
    icp_query_execution_gate_safe_raw-c8c66378)
      echo "crates/verified-core/src/kasane_precompiles/icp_query_execution_gate.rs"
      ;;
    icp_update_status_consumes_capacity_raw-882a4379)
      echo "crates/verified-core/src/kasane_precompiles/icp_update_status_consumes_capacity.rs"
      ;;
    icp_update_capacity_accepts_raw-9d22db3f)
      echo "crates/verified-core/src/kasane_precompiles/icp_update_capacity_accepts.rs"
      ;;
    *)
      fail "unknown specgen target: $1"
      ;;
  esac
}

check_specgen_targets() {
  local targets=(
    "compact_icp_query_input_safe_raw-8605da94"
    "icp_query_update_kind_rejected_raw-b2b79d8e"
    "icp_query_gas_observation_safe_raw-9b7ab62f"
    "icp_precompile_allowlist_entry_safe_raw-0ba30703"
    "icp_query_execution_gate_safe_raw-c8c66378"
    "icp_update_status_consumes_capacity_raw-882a4379"
    "icp_update_capacity_accepts_raw-9d22db3f"
  )

  log "check specgen targets, accepted specs, and status"
  require_file "spec/targets.toml"

  local target
  for target in "${targets[@]}"; do
    local target_file
    target_file="$(target_file_for_slug "${target}")"
    require_contains "spec/targets.toml" "slug = \"${target}\""
    require_contains "spec/targets.toml" "file = \"${target_file}\""
    require_file "spec/accepted/${target}.json"
    require_file "spec/accepted/${target}.md"
    require_contains "spec/accepted/${target}.json" "\"slug\": \"${target}\""
    require_contains "spec/accepted/${target}.json" "\"file\": \"${target_file}\""
    require_specgen_status_pass "${target}"
  done
}

check_specgen_verify_evidence() {
  local targets=(
    "compact_icp_query_input_safe_raw-8605da94"
    "icp_query_update_kind_rejected_raw-b2b79d8e"
    "icp_query_gas_observation_safe_raw-9b7ab62f"
    "icp_precompile_allowlist_entry_safe_raw-0ba30703"
    "icp_query_execution_gate_safe_raw-c8c66378"
    "icp_update_status_consumes_capacity_raw-882a4379"
    "icp_update_capacity_accepts_raw-9d22db3f"
  )

  log "check generated Verus evidence"
  local target
  for target in "${targets[@]}"; do
    local report="spec/reports/${target}_verify.json"
    require_file "${report}"
    require_contains "${report}" "\"slug\": \"${target}\""
    require_contains "${report}" "\"semantic_hash\":"
    require_contains "${report}" "\"source_hash\":"
    require_contains "${report}" "\"spec_hash\":"
    require_contains "${report}" "\"target_hash\":"
    require_contains "${report}" "\"contract_hash\":"
    require_contains "${report}" "\"result\": \"success\""
  done
}

check_specgen_artifacts() {
  check_specgen_targets
  check_specgen_verify_evidence
}

check_bidi_controls() {
  log "check bidi control characters"
  if rg -n -P '[\x{202A}-\x{202E}\x{2066}-\x{2069}]' README.md crates docs spec scripts; then
    fail "bidi control characters found"
  fi
}

prepare_pocket_ic_bin() {
  local candidates=()

  if [[ -n "${POCKET_IC_BIN:-}" ]]; then
    candidates+=("${POCKET_IC_BIN}")
  fi
  candidates+=("crates/evm-rpc-e2e/pocket-ic")

  local candidate
  for candidate in "${candidates[@]}"; do
    if [[ -x "${candidate}" ]] && "${candidate}" --version 2>/dev/null | grep -Eq '^pocket-ic-server 12\.'; then
      export POCKET_IC_BIN="${candidate}"
      log "use PocketIC binary: ${POCKET_IC_BIN}"
      return
    fi
  done

  fail "PocketIC server 12.x is required for evm-rpc-e2e; set POCKET_IC_BIN to a compatible binary"
}

run_rust_checks() {
  log "run Verus verification"
  scripts/verify-verus.sh

  log "run verified-core tests"
  cargo test -p verified-core

  log "run ICP query parser/model tests"
  cargo test -p ic-evm-core icp_query_precompile

  log "run ICP query async precompile tests"
  cargo test -p ic-evm-core kasane_precompiles_query

  log "run gateway allowlist boundary tests"
  cargo test -p ic-evm-gateway query_precompile_allow

  log "build gateway wasm for PocketIC ICP query precompile tests"
  cargo build -p ic-evm-gateway --target wasm32-unknown-unknown --release

  log "run ICP query PocketIC E2E tests"
  prepare_pocket_ic_bin
  cargo test --manifest-path crates/evm-rpc-e2e/Cargo.toml query_precompile_

  log "run workspace check"
  cargo check --workspace

  log "run rustfmt check for PR #81 Rust files"
  rustfmt --edition 2021 --check \
    crates/verified-core/src/kasane_precompiles.rs \
    crates/evm-core/src/kasane_precompiles.rs \
    crates/evm-core/src/kasane_precompiles_tests.rs \
    crates/evm-core/tests/common/mod.rs \
    crates/evm-core/tests/kasane_precompiles_query.rs \
    crates/evm-rpc-e2e/tests/rpc_compat_e2e.rs \
    crates/ic-evm-gateway/src/lib.rs \
    crates/ic-evm-gateway/src/tests.rs

  log "run whitespace diff check"
  git diff --check
}

check_specgen_artifacts
check_bidi_controls
run_rust_checks

log "ok"
