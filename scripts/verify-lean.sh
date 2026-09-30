#!/usr/bin/env bash
# Verify EVM adapter models and compare pure-function boundary cases with Rust.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
cd "${repo_root}"
# A changed source needs a new correspondence review, even when vectors pass.
shasum -a 256 -c proofs/evm/model-sources.sha256
cd "${repo_root}/proofs/evm"
lake build
lake env lean -DwarningAsError=true Audit.lean
rustc --edition=2021 rust_vectors.rs -o .lake/rust-vectors
.lake/rust-vectors > .lake/rust-vectors.txt
lake exe vectors > .lake/lean-vectors.txt
diff -u .lake/rust-vectors.txt .lake/lean-vectors.txt
echo "[verify-lean] proofs, axiom audit, and $(wc -l < .lake/rust-vectors.txt | tr -d ' ') Rust/Lean vectors passed"
