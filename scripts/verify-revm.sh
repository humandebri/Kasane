#!/usr/bin/env bash
# Reproducible local evidence for the pinned EVM journal/REVERT scope.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
cd "${repo_root}"
python3 scripts/check_revm_verification_profile.py
shasum -a 256 -c proofs/evm/fixtures/ethereum/SHA256SUMS
bash scripts/verify-lean.sh
KASANE_JOURNAL_VECTORS="${repo_root}/proofs/evm/.lake/revm-journal.txt" \
  cargo test --locked -p ic-evm-core --test revm_journal \
    --test revm_state_fixtures --test kasane_nested_revert
(
  cd proofs/evm
  lake exe journal-vectors > .lake/lean-journal.txt
  diff -u .lake/revm-journal.txt .lake/lean-journal.txt
)
echo "[verify-revm] 9 journal traces / 45 observations, 38 Prague state vectors, 12 Kasane CALL cases passed"
