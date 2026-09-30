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
KASANE_JOURNAL_VECTORS="${repo_root}/proofs/evm/.lake/revm-journal.txt" \
  cargo test --locked -p ic-evm-core --test revm_journal \
    --test revm_state_fixtures --test kasane_nested_revert
cargo test --locked -p ic-evm-core --test kasane_precompiles_query \
  icp_update_intent_reverted_subcall_does_not_consume_capacity
(
  cd proofs/evm
  lake exe journal-vectors > .lake/lean-journal.txt
  diff -u .lake/revm-journal.txt .lake/lean-journal.txt
)
echo "[verify-revm] 9 Lean journal traces, 486 short Rust traces, 38 Prague vectors, Kasane CALL/CREATE/SELFDESTRUCT and precompile rollback passed"
