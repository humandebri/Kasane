#!/usr/bin/env bash
# where: shared GitHub-equivalent CI entrypoint
# what: GitHub Actions checks job と同じ実処理を 1 箇所で実行する
# why: workflow と ci-local の手順差分をなくして常に同期させるため
set -euo pipefail

CI_LOCAL_SKIP_TOOL_INSTALL="${CI_LOCAL_SKIP_TOOL_INSTALL:-0}"
EVM_CANISTER_ID="${EVM_CANISTER_ID:-aaaaa-aa}"
CANBENCH_MAX_REGRESSION_PCT="${CANBENCH_MAX_REGRESSION_PCT:-2.0}"
CANBENCH_TARGET_IMPROVEMENT_PCT="${CANBENCH_TARGET_IMPROVEMENT_PCT:-5.0}"

snapshot_dir="$(mktemp -d "${TMPDIR:-/tmp}/kasane-supply-chain.XXXXXX")"
trap 'rm -rf "${snapshot_dir}"' EXIT

if ! command -v node >/dev/null 2>&1; then
  echo "[ci-github-equivalent] node is required" >&2
  exit 1
fi

if ! command -v forge >/dev/null 2>&1; then
  echo "[ci-github-equivalent] forge is required" >&2
  exit 1
fi

scripts/check_rng_paths.sh
scripts/check_getrandom_wasm_features.sh
scripts/check_did_sync.sh
scripts/check_gateway_api_compat_baseline.sh
scripts/check_gateway_matrix_sync.sh
scripts/check_alloy_isolation.sh
scripts/check_precompile_feature_isolation.sh

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "[ci-github-equivalent] deny OP stack references"
DENY_PATTERN='op-revm|op_revm|op-node|op-geth|optimism|superchain|OpDeposit|L1BlockInfo'
if grep -RInE "$DENY_PATTERN" \
  --exclude-dir=.git \
  --exclude-dir=target \
  --exclude-dir=vendor \
  --exclude-dir=node_modules \
  --exclude='ci-local.sh' \
  --exclude='ci_github_equivalent.sh' \
  --exclude='pr0-differential-runbook.md' \
  crates docs scripts README.md Cargo.toml; then
  echo "[ci-github-equivalent] forbidden OP stack reference found" >&2
  exit 1
fi

if ! command -v cargo-deny >/dev/null 2>&1 || ! command -v cargo-audit >/dev/null 2>&1; then
  if [[ "${CI_LOCAL_SKIP_TOOL_INSTALL}" == "1" ]]; then
    echo "[ci-github-equivalent] missing cargo-deny/cargo-audit and CI_LOCAL_SKIP_TOOL_INSTALL=1" >&2
    exit 1
  fi
  echo "[ci-github-equivalent] install supply-chain tools"
  cargo install --locked cargo-deny cargo-audit
fi

cargo deny check
cargo audit --deny warnings --ignore RUSTSEC-2024-0388 --ignore RUSTSEC-2024-0436

cargo metadata --locked --format-version 1 > "${snapshot_dir}/cargo-metadata.sbom.json"
find vendor/revm -type f -print0 | sort -z | xargs -0 sha256sum > "${snapshot_dir}/vendor-revm.sha256"
find vendor/ark-relations -type f -print0 | sort -z | xargs -0 sha256sum > "${snapshot_dir}/vendor-ark-relations.sha256"

# evm-rpc-e2e uses Foundry artifacts via include_str!, so generate them
# before compiling the Rust tests in clean CI environments.
(cd tools/wrapper-vite/contracts && forge build)

cargo test -p evm-db -p ic-evm-core -p ic-evm-gateway --locked --lib --tests
cargo test --manifest-path crates/evm-rpc-e2e/Cargo.toml --no-run --locked
cargo build --release --target wasm32-unknown-unknown -p wrap-canister -p mock-wrap-canister -p ic-evm-gateway --locked

. scripts/prepare_ci_icrc1_ledger_wasm.sh
cargo test --manifest-path crates/evm-rpc-e2e/Cargo.toml --test wrap_unwrap_flow_e2e --locked -- --test-threads=1

(cd tools/wrapper-vite/contracts && forge test -vv)

if [[ ! -d tools/rpc-gateway/node_modules ]]; then
  (cd tools/rpc-gateway && npm ci)
fi
(cd tools/rpc-gateway && EVM_CANISTER_ID="${EVM_CANISTER_ID}" npm test && npm run build)

CANBENCH_MAX_REGRESSION_PCT="${CANBENCH_MAX_REGRESSION_PCT}" \
CANBENCH_TARGET_IMPROVEMENT_PCT="${CANBENCH_TARGET_IMPROVEMENT_PCT}" \
scripts/run_canbench_guard.sh
