#!/usr/bin/env bash
# Reproducible local evidence for the pinned EVM journal/REVERT scope.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
cd "${repo_root}"
# The nested PocketIC E2E crate has a different Cargo.lock. Keep its build
# artifacts from supplying incompatible dependency types to this proof gate.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${repo_root}/target/evm-proof}"
python3 scripts/check_revm_verification_profile.py
shasum -a 256 -c proofs/evm/fixtures/ethereum/SHA256SUMS
bash scripts/verify-lean.sh
rm -f proofs/evm/.lake/rust-sizes.txt proofs/evm/.lake/rust-authorization.txt
KASANE_SIZE_VECTORS="${repo_root}/proofs/evm/.lake/rust-sizes.txt" \
KASANE_AUTHORIZATION_VECTORS="${repo_root}/proofs/evm/.lake/rust-authorization.txt" \
  cargo test --locked -p ic-evm-core --lib lean_
cargo test --locked -p ic-evm-core --lib \
  revm_exec::tests::oversized_execution_does_not_modify_shared_cache_or_stable_state
(
  cd proofs/evm
  cat .lake/rust-sizes.txt .lake/rust-authorization.txt > .lake/rust-adapters.txt
  lake exe adapter-vectors > .lake/lean-adapters.txt
  diff -u .lake/rust-adapters.txt .lake/lean-adapters.txt
)
KASANE_JOURNAL_VECTORS="${repo_root}/proofs/evm/.lake/revm-journal.txt" \
  cargo test --locked -p ic-evm-core --test revm_journal \
    --test revm_state_fixtures --test kasane_nested_revert
cargo test --locked -p ic-evm-core --test kasane_precompiles_query \
  icp_update_intent_reverted_subcall_does_not_consume_capacity
cargo test --locked -p ic-evm-core --test kasane_precompiles_query \
  withdrawal_precompiles_reject_delegated_call_contexts
(
  cd proofs/evm
  lake exe journal-vectors > .lake/lean-journal.txt
  diff -u .lake/revm-journal.txt .lake/lean-journal.txt
)
echo "[verify-revm] 402 adapter vectors, rejected-result retry, 9 Lean journal traces, 486 short Rust traces, 38 Prague vectors, Kasane CALL/CREATE/SELFDESTRUCT and precompile authorization/rollback passed"
