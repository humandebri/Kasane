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

check_specgen_targets() {
  local targets=(
    "compact_icp_query_input_safe_raw-8482ca59"
    "icp_query_update_kind_rejected_raw-4de9db5f"
    "icp_query_gas_observation_safe_raw-ae357da2"
    "icp_query_allowlist_entry_safe_raw-744d724a"
    "icp_query_execution_gate_safe_raw-c8c66378"
  )

  log "check specgen targets and accepted specs"
  require_file "spec/targets.toml"

  local target
  for target in "${targets[@]}"; do
    require_contains "spec/targets.toml" "slug = \"${target}\""
    require_file "spec/accepted/${target}.json"
    require_contains "spec/accepted/${target}.json" "\"slug\": \"${target}\""
  done
}

check_specgen_test_evidence() {
  local evidence=(
    "spec/reports/compact_icp_query_input_safe_raw-8482ca59_tests.json"
    "spec/reports/icp_query_update_kind_rejected_raw-4de9db5f_tests.json"
    "spec/reports/icp_query_allowlist_entry_safe_raw-744d724a_tests.json"
    "spec/reports/icp_query_execution_gate_safe_raw-c8c66378_tests.json"
  )

  log "check generated specgen test evidence"
  local report
  for report in "${evidence[@]}"; do
    require_file "${report}"
    require_contains "${report}" "\"result\": \"success\""
  done
}

check_specgen_gate_report() {
  local report="spec/reports/pr81-verification-report.md"

  log "check specgen diagnostic report"
  require_file "${report}"
  require_contains "${report}" "- check_result: fail"
  require_contains "${report}" "## missing targets"
  require_contains "${report}" "icp_query_gas_observation_safe_raw-ae357da2"
  require_contains "${report}" "missing_test_evidence_report"
  require_contains "${report}" "missing_verify_report"
}

check_specgen_artifacts() {
  check_specgen_targets
  check_specgen_test_evidence
  check_specgen_gate_report
}

run_rust_checks() {
  log "run Verus verification"
  scripts/verify-verus.sh

  log "run verified-core tests"
  cargo test -p verified-core

  log "run ICP query parser/model tests"
  cargo test -p ic-evm-core icp_query_precompile

  log "run ICP query async precompile tests"
  cargo test -p ic-evm-core wrap_precompile_query

  log "run gateway allowlist boundary tests"
  cargo test -p ic-evm-gateway query_precompile_allow

  log "run workspace check"
  cargo check --workspace

  log "run rustfmt check for PR #81 Rust files"
  rustfmt --edition 2021 --check \
    crates/verified-core/src/wrap_precompile.rs \
    crates/evm-core/src/wrap_precompile.rs \
    crates/evm-core/src/wrap_precompile_tests.rs \
    crates/evm-core/tests/common/mod.rs \
    crates/evm-core/tests/wrap_precompile_query.rs \
    crates/ic-evm-gateway/src/lib.rs \
    crates/ic-evm-gateway/src/tests.rs

  log "run whitespace diff check"
  git diff --check
}

check_specgen_artifacts
run_rust_checks

log "ok"
