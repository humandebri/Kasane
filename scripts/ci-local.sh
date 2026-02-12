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

  scripts/check_rng_paths.sh
  scripts/check_getrandom_wasm_features.sh
  scripts/check_did_sync.sh
  scripts/check_legacy_rpc_removed.sh
  scripts/check_alloy_isolation.sh

  echo "[phase=${CURRENT_PHASE}] deny OP stack references"
  DENY_PATTERN='op-revm|op_revm|op-node|op-geth|optimism|superchain|OpDeposit|L1BlockInfo'
  if grep -RInE "$DENY_PATTERN" \
    --exclude-dir=.git \
    --exclude-dir=target \
    --exclude-dir=vendor \
    --exclude-dir=node_modules \
    --exclude='scripts/ci-local.sh' \
    --exclude='scripts/ci-local_github_equivalent.sh' \
    --exclude='docs/ops/pr0-differential-runbook.md' \
    crates docs scripts README.md Cargo.toml; then
    echo "[phase=${CURRENT_PHASE}] forbidden OP stack reference found" >&2
    exit 1
  fi

  if ! command -v cargo-deny >/dev/null 2>&1 || ! command -v cargo-audit >/dev/null 2>&1; then
    if [[ "${CI_LOCAL_SKIP_TOOL_INSTALL}" == "1" ]]; then
      echo "[phase=${CURRENT_PHASE}] missing cargo-deny/cargo-audit and CI_LOCAL_SKIP_TOOL_INSTALL=1" >&2
      exit 1
    fi
    echo "[phase=${CURRENT_PHASE}] install supply-chain tools"
    cargo install --locked cargo-deny cargo-audit
  fi

  cargo deny check
  cargo audit --deny warnings --ignore RUSTSEC-2024-0388 --ignore RUSTSEC-2024-0436

  cargo metadata --locked --format-version 1 > cargo-metadata.sbom.json
  find vendor/revm -type f -print0 | sort -z | xargs -0 sha256sum > vendor-revm.sha256
  find vendor/ark-relations -type f -print0 | sort -z | xargs -0 sha256sum > vendor-ark-relations.sha256

  cargo test -p evm-db -p ic-evm-core -p ic-evm-wrapper --locked --lib --tests
  cargo test --manifest-path crates/evm-rpc-e2e/Cargo.toml --no-run --locked

  scripts/run_canbench_guard.sh
  scripts/release_wasm_guard.sh

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
