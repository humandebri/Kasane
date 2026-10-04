#!/usr/bin/env bash
# Extract fixed revm Gas methods using the production root Cargo.lock.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
cd "${repo_root}"
shasum -a 256 -c proofs/extraction/revm-gas-sources.sha256
python3 - <<'PY'
import hashlib, json, pathlib
profile = json.load(open('proofs/extraction/tool-patches/build-profile.json'))
for filename, expected in profile['experimental_binary_sha256'].items():
    assert hashlib.sha256(pathlib.Path(filename).read_bytes()).hexdigest() == expected, filename
PY
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export DYLD_LIBRARY_PATH="${repo_root}/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:${DYLD_LIBRARY_PATH}}"
export CARGO_TARGET_DIR="${repo_root}/.local/proof-tools/revm-root-target"
extract_tmp="$(mktemp -d "${TMPDIR:-/tmp}/kasane-revm-gas.XXXXXX")"
trap 'rm -rf "${extract_tmp}"' EXIT
target/debug/charon cargo --preset=aeneas --mir optimized --error-on-warnings \
  --start-from 'revm_interpreter::gas::Gas::record_cost' \
  --start-from 'revm_interpreter::gas::Gas::record_cost_unsafe' \
  --include 'revm_interpreter::gas::_::record_cost' \
  --include 'revm_interpreter::gas::_::record_cost_unsafe' \
  --include 'revm_interpreter::gas::Gas' --include 'revm_interpreter::gas::MemoryGas' \
  --dest-file "${extract_tmp}/kasane-revm-gas.llbc" \
  -- --manifest-path crates/evm-core/Cargo.toml --lib --locked
.local/proof-tools/aeneas-source/src/_build/default/main.exe -backend lean \
  -dest "${extract_tmp}" -namespace RevmGas -use-lean-modules false \
  -abort-on-error -warnings-as-errors "${extract_tmp}/kasane-revm-gas.llbc"
diff -u proofs/extraction/u256-lean/RevmGasGenerated.lean "${extract_tmp}/Kasane-revm-gas.lean"
cd proofs/extraction/u256-lean
lake build
lake env lean -DwarningAsError=true RevmGasCorrespondence.lean
lake env lean -DwarningAsError=true Audit.lean
lake env leanchecker RevmGasCorrespondence
echo '[verify-revm-gas-experimental] fixed root-profile extraction, all-input Gas correspondence, audit and kernel recheck passed'
