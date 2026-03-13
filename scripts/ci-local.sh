#!/usr/bin/env bash
# where: local dev CI entrypoint
# what: run GitHub-equivalent checks and optional local smoke in separated phases
# why: isolate failure domain between CI parity checks and heavy local integration smoke
set -euo pipefail

CI_LOCAL_MODE="${CI_LOCAL_MODE:-all}"
NETWORK="${NETWORK:-local}"
ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME:-ci-local}"
CI_LOCAL_SKIP_TOOL_INSTALL="${CI_LOCAL_SKIP_TOOL_INSTALL:-0}"
CURRENT_PHASE="setup"

phase_fail() {
  local code="$?"
  echo "[phase=${CURRENT_PHASE}] failed (exit=${code})" >&2
  exit "${code}"
}
trap phase_fail ERR

run_github_equivalent_phase() {
  CURRENT_PHASE="github"
  echo "[phase=${CURRENT_PHASE}] start"
  local default_cargo_home
  default_cargo_home="${HOME}/.cargo"
  local fallback_cargo_home
  fallback_cargo_home="${XDG_CACHE_HOME:-${HOME}/.cache}/kasane-cargo-home"

  if [[ -z "${CARGO_HOME:-}" && ( ! -d "${default_cargo_home}" || ! -w "${default_cargo_home}" ) ]]; then
    mkdir -p "${fallback_cargo_home}"
    if [[ ! -w "${fallback_cargo_home}" ]]; then
      echo "[phase=${CURRENT_PHASE}] fallback CARGO_HOME is not writable: ${fallback_cargo_home}" >&2
      exit 1
    fi
    CARGO_HOME="${fallback_cargo_home}"
    export CARGO_HOME
    echo "[phase=${CURRENT_PHASE}] CARGO_HOME is not writable, fallback to ${CARGO_HOME}"
  fi

  scripts/ci_github_equivalent.sh

  echo "[phase=${CURRENT_PHASE}] done"
}

run_local_smoke_phase() {
  CURRENT_PHASE="smoke"
  echo "[phase=${CURRENT_PHASE}] start"

  NETWORK="${NETWORK}" \
  ICP_IDENTITY_NAME="${ICP_IDENTITY_NAME}" \
  scripts/local_indexer_smoke.sh

  echo "[phase=${CURRENT_PHASE}] done"
}

case "${CI_LOCAL_MODE}" in
  all)
    run_github_equivalent_phase
    run_local_smoke_phase
    ;;
  github)
    run_github_equivalent_phase
    ;;
  smoke)
    run_local_smoke_phase
    ;;
  *)
    echo "invalid CI_LOCAL_MODE: ${CI_LOCAL_MODE} (expected: all|github|smoke)" >&2
    exit 2
    ;;
esac
