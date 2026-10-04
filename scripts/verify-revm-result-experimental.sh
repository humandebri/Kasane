#!/usr/bin/env bash
# Extract fixed revm InstructionResult methods using the production root Cargo.lock.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
cd "${repo_root}"
shasum -a 256 -c proofs/extraction/revm-result-sources.sha256
python3 - <<'PY'
import hashlib, json, pathlib
profile = json.load(open('proofs/extraction/tool-patches/build-profile.json'))
for filename, expected in profile['experimental_binary_sha256'].items():
    assert hashlib.sha256(pathlib.Path(filename).read_bytes()).hexdigest() == expected, filename
PY
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export DYLD_LIBRARY_PATH="${repo_root}/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:${DYLD_LIBRARY_PATH}}"
export CARGO_TARGET_DIR="${repo_root}/.local/proof-tools/revm-root-target"
extract_tmp="$(mktemp -d "${TMPDIR:-/tmp}/kasane-revm-result.XXXXXX")"
trap 'rm -rf "${extract_tmp}"' EXIT
methods=()
for method in is_ok is_revert is_error is_ok_or_revert; do
  methods+=(--start-from "revm_interpreter::instruction_result::InstructionResult::${method}")
  methods+=(--include "revm_interpreter::instruction_result::_::${method}")
done
target/debug/charon cargo --preset=aeneas --mir optimized --error-on-warnings \
  "${methods[@]}" --include 'revm_interpreter::instruction_result::InstructionResult' \
  --dest-file "${extract_tmp}/kasane-revm-result.llbc" \
  -- --manifest-path crates/evm-core/Cargo.toml --lib --locked
.local/proof-tools/aeneas-source/src/_build/default/main.exe -backend lean \
  -dest "${extract_tmp}" -namespace RevmResult -use-lean-modules false \
  -abort-on-error -warnings-as-errors "${extract_tmp}/kasane-revm-result.llbc"
diff -u proofs/extraction/u256-lean/RevmResultGenerated.lean "${extract_tmp}/Kasane-revm-result.lean"
cd proofs/extraction/u256-lean
lake build
lake env lean -DwarningAsError=true RevmResultCorrespondence.lean
lake env lean -DwarningAsError=true Audit.lean
lake env leanchecker RevmResultCorrespondence
echo '[verify-revm-result-experimental] fixed root-profile extraction, all 32 result classifications, audit and kernel recheck passed'
