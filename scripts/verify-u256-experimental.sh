#!/usr/bin/env bash
# Experimental patched-tool extraction; separate from the released proof gate.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
cd "${repo_root}"
shasum -a 256 -c proofs/extraction/u256-sources.sha256
python3 - <<'BINARY_HASHES'
import hashlib, json, pathlib
profile = json.load(open("proofs/extraction/tool-patches/build-profile.json"))
for filename, expected in profile["experimental_binary_sha256"].items():
    assert hashlib.sha256(pathlib.Path(filename).read_bytes()).hexdigest() == expected, filename
BINARY_HASHES
production_profile="$(cargo tree -p ruint --locked --depth 0 --format '{p} {f}')"
probe_profile="$(cargo tree -p ruint --manifest-path proofs/extraction/u256-rust/Cargo.toml --locked --depth 0 --format '{p} {f}')"
[[ "${production_profile}" == 'ruint v1.20.0 alloc,alloy-rlp,serde,std' ]]
[[ "${probe_profile}" == "${production_profile}" ]]
metadata_tmp="$(mktemp "${TMPDIR:-/tmp}/kasane-u256-metadata.XXXXXX")"
cargo metadata --manifest-path proofs/extraction/u256-rust/Cargo.toml --locked \
  --format-version 1 > "${metadata_tmp}"
python3 - "${metadata_tmp}" <<'RUINT_SOURCES'
import hashlib, json, pathlib, sys
metadata = json.load(open(sys.argv[1]))
package = next(p for p in metadata["packages"] if p["name"] == "ruint")
assert package["version"] == "1.20.0"
base = pathlib.Path(package["manifest_path"]).parent
profile = json.load(open("proofs/extraction/tool-patches/build-profile.json"))
for filename, expected in profile["ruint_sources_sha256"].items():
    assert hashlib.sha256((base / filename).read_bytes()).hexdigest() == expected, filename
RUINT_SOURCES
rm "${metadata_tmp}"
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export DYLD_LIBRARY_PATH="${repo_root}/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:${DYLD_LIBRARY_PATH}}"
export CARGO_TARGET_DIR="${repo_root}/.local/proof-tools/u256-target"
extract_tmp="$(mktemp -d "${TMPDIR:-/tmp}/kasane-u256.XXXXXX")"
trap 'rm -rf "${extract_tmp}"' EXIT
includes=()
for item in 'ruint::Uint' 'ruint::add::_::wrapping_add' 'ruint::add::_::wrapping_sub' \
  'ruint::add::_::overflowing_add' 'ruint::add::_::overflowing_sub' \
  'ruint::_::masked' 'ruint::_::apply_mask' 'ruint::_::SHOULD_MASK' \
  'ruint::_::from_limbs' 'ruint::_::LIMBS' 'ruint::nlimbs' 'ruint::_::ZERO' \
  'ruint::_::MASK' 'ruint::mask' 'core::num::_::unchecked_add' \
  'core::num::_::div_ceil' 'core::ub_checks::check_language_ub' \
  'ruint::algorithms::add::carrying_add' 'ruint::algorithms::add::borrowing_sub'; do
  includes+=(--include "${item}")
done
target/debug/charon cargo --preset=aeneas --mir optimized \
  --rustc-arg=-Copt-level=3 --rustc-arg=-Cdebug-assertions=no --rustc-arg=-Zub-checks=no \
  --start-from 'kasane_u256_extracted' "${includes[@]}" \
  --dest-file "${extract_tmp}/kasane_u256_no_axioms.llbc" \
  -- --manifest-path proofs/extraction/u256-rust/Cargo.toml --locked
.local/proof-tools/aeneas-source/src/_build/default/main.exe -backend lean \
  -dest "${extract_tmp}" -namespace U256Extracted -use-lean-modules false \
  -abort-on-error -warnings-as-errors "${extract_tmp}/kasane_u256_no_axioms.llbc"
diff -u proofs/extraction/u256-lean/U256Generated.lean "${extract_tmp}/KasaneU256NoAxioms.lean"
cd proofs/extraction/u256-lean
lake build
lake env lean -DwarningAsError=true U256Correspondence.lean
lake env lean -DwarningAsError=true U256Loops.lean
lake env lean -DwarningAsError=true U256Arithmetic.lean
lake env lean -DwarningAsError=true Audit.lean
lake env leanchecker U256Correspondence
lake env leanchecker U256Loops
lake env leanchecker U256Arithmetic
echo '[verify-u256-experimental] regeneration, all-input U256 add/sub arithmetic correspondence, audit and kernel recheck passed; EVM and native/Wasm correspondence remain unproved'
