#!/usr/bin/env bash
# Extract fixed revm Stack readers using the production root Cargo.lock.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
cd "${repo_root}"
shasum -a 256 -c proofs/extraction/revm-stack-sources.sha256
python3 - <<'PY'
import hashlib, json, pathlib, subprocess
profile = json.load(open('proofs/extraction/tool-patches/build-profile.json'))
sysroot = pathlib.Path(subprocess.check_output(['rustc','+nightly-2026-09-17','--print','sysroot'],text=True).strip())
stdvec = sysroot/'lib/rustlib/src/rust/library/alloc/src/vec/mod.rs'
assert hashlib.sha256(stdvec.read_bytes()).hexdigest() == profile['stack_proofs']['stdlib_vec_source_sha256']
for filename, expected in profile['experimental_binary_sha256'].items():
    assert hashlib.sha256(pathlib.Path(filename).read_bytes()).hexdigest() == expected, filename
PY
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export DYLD_LIBRARY_PATH="${repo_root}/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:${DYLD_LIBRARY_PATH}}"
export CARGO_TARGET_DIR="${repo_root}/.local/proof-tools/revm-root-target"
extract_tmp="$(mktemp -d "${TMPDIR:-/tmp}/kasane-revm-stack.XXXXXX")"
trap 'rm -rf "${extract_tmp}"' EXIT
methods=()
for method in len is_empty peek; do
  methods+=(--start-from "revm_interpreter::interpreter::stack::Stack::${method}")
  methods+=(--include "revm_interpreter::interpreter::stack::_::${method}")
done
target/debug/charon cargo --preset=aeneas --mir optimized --error-on-warnings \
  "${methods[@]}" --include 'revm_interpreter::instruction_result::InstructionResult' \
  --include 'revm_interpreter::interpreter::stack::Stack' --include 'ruint::Uint' \
  --include 'alloc::vec::_::is_empty' \
  --dest-file "${extract_tmp}/kasane-revm-stack.llbc" \
  -- --manifest-path crates/evm-core/Cargo.toml --lib --locked
.local/proof-tools/aeneas-source/src/_build/default/main.exe -backend lean \
  -dest "${extract_tmp}" -namespace RevmStack -use-lean-modules false \
  -abort-on-error -warnings-as-errors "${extract_tmp}/kasane-revm-stack.llbc"
diff -u proofs/extraction/operator-lean/StackGenerated.lean "${extract_tmp}/Kasane-revm-stack.lean"
cd proofs/extraction/operator-lean
lake build
lake env lean -DwarningAsError=true StackCorrespondence.lean
lake env lean -DwarningAsError=true Audit.lean
lake env leanchecker StackCorrespondence
echo '[verify-revm-stack-experimental] production root-profile len/is_empty/peek regeneration, all-input value/underflow correspondence, audit and kernel recheck passed; mutating stack and full EVM/native/Wasm correspondence remain unproved'
