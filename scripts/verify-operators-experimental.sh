#!/usr/bin/env bash
# Experimental patched-tool extraction; separate from the released proof gate.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
cd "${repo_root}"
shasum -a 256 -c proofs/extraction/bitwise-sources.sha256
shasum -a 256 -c proofs/extraction/operator-backend-sources.sha256
python3 - <<'BINARY_HASHES'
import hashlib, json, pathlib
profile = json.load(open("proofs/extraction/tool-patches/build-profile.json"))
for filename, expected in profile["experimental_binary_sha256"].items():
    assert hashlib.sha256(pathlib.Path(filename).read_bytes()).hexdigest() == expected, filename
BINARY_HASHES
production_profile="$(cargo tree -p ruint --locked --depth 0 --format '{p} {f}')"
probe_profile="$(cargo tree -p ruint --manifest-path proofs/extraction/bitwise-rust/Cargo.toml --locked --depth 0 --format '{p} {f}')"
[[ "${production_profile}" == 'ruint v1.20.0 alloc,alloy-rlp,serde,std' ]]
[[ "${probe_profile}" == "${production_profile}" ]]
metadata_tmp="$(mktemp "${TMPDIR:-/tmp}/kasane-bitwise-metadata.XXXXXX")"
cargo metadata --manifest-path proofs/extraction/bitwise-rust/Cargo.toml --locked \
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
extract_tmp="$(mktemp -d "${TMPDIR:-/tmp}/kasane-operators.XXXXXX")"
trap 'rm -rf "${extract_tmp}"' EXIT
includes=()
for item in 'ruint::Uint' 'ruint::bits::_::bitand' 'ruint::bits::_::bitor' \
  'ruint::bits::_::bitxor' 'ruint::bits::_::bitand_assign' 'ruint::bits::_::bitor_assign' 'ruint::bits::_::bitxor_assign' 'core::ops::bit::_::bitand_assign' 'core::ops::bit::_::bitor_assign' 'core::ops::bit::_::bitxor_assign' 'core::num::_::unchecked_add' 'core::ub_checks::check_language_ub'; do
  includes+=(--include "${item}")
done
roots=()
for method in operator_and operator_or operator_xor; do
  roots+=(--start-from "kasane_bitwise_extracted::${method}")
done
target/debug/charon cargo --preset=aeneas --mir optimized \
  --rustc-arg=-Copt-level=3 --rustc-arg=-Cdebug-assertions=no --rustc-arg=-Zub-checks=no \
  "${roots[@]}" "${includes[@]}" \
  --dest-file "${extract_tmp}/kasane-bitwise-operators.llbc" \
  -- --manifest-path proofs/extraction/bitwise-rust/Cargo.toml --locked
.local/proof-tools/aeneas-source/src/_build/default/main.exe -backend lean \
  -dest "${extract_tmp}" -namespace BitwiseOperators -use-lean-modules false \
  -abort-on-error -warnings-as-errors "${extract_tmp}/kasane-bitwise-operators.llbc"
diff -u proofs/extraction/operator-lean/OperatorsGenerated.lean "${extract_tmp}/Kasane-bitwise-operators.lean"
cd proofs/extraction/operator-lean
lake build
lake env lean -DwarningAsError=true OperatorCorrespondence.lean
lake env lean -DwarningAsError=true Audit.lean
lake env leanchecker OperatorCorrespondence
echo '[verify-operators-experimental] actual U256 AND/OR/XOR trait operators regenerated; all-input loop termination and all-bit correspondence, audit and kernel recheck passed; EVM stack/gas/exception and native/Wasm correspondence remain unproved'
