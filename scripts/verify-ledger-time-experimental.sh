#!/usr/bin/env bash
# Extract unchanged fixed-release ledger TimeStamp methods in a checked-arithmetic profile.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
cd "${repo_root}"
shasum -a 256 -c proofs/extraction/ledger-time-sources.sha256
python3 - <<'PY'
import hashlib, json, pathlib
profile = json.load(open('proofs/extraction/tool-patches/build-profile.json'))
ledger = json.load(open('proofs/external/ledger-profile.json'))
source = next(s for s in ledger['reviewed_sources'] if s['path'].endswith('/timestamp.rs'))
assert hashlib.sha256(pathlib.Path('proofs/extraction/ledger-time-rust/timestamp.rs').read_bytes()).hexdigest() == source['sha256']
for filename, expected in profile['experimental_binary_sha256'].items():
    assert hashlib.sha256(pathlib.Path(filename).read_bytes()).hexdigest() == expected, filename
PY
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export DYLD_LIBRARY_PATH="${repo_root}/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:${DYLD_LIBRARY_PATH}}"
export CARGO_TARGET_DIR="${repo_root}/.local/proof-tools/ledger-timestamp-target"
extract_tmp="$(mktemp -d "${TMPDIR:-/tmp}/kasane-ledger-time.XXXXXX")"
trap 'rm -rf "${extract_tmp}"' EXIT
# Overflow failures below are proved for this explicit profile, not the release Wasm.
export RUSTFLAGS='-Coverflow-checks=yes'
methods=()
for method in from_nanos get_nanos new_time; do
  methods+=(--start-from "kasane_ledger_timestamp_probe::${method}")
done
target/debug/charon cargo --preset=aeneas --mir optimized --error-on-warnings \
  "${methods[@]}" \
  --dest-file "${extract_tmp}/kasane-ledger-time-constructors.llbc" \
  -- --manifest-path proofs/extraction/ledger-time-rust/Cargo.toml --lib --locked
.local/proof-tools/aeneas-source/src/_build/default/main.exe -backend lean \
  -dest "${extract_tmp}" -namespace LedgerTime -use-lean-modules false \
  -abort-on-error -warnings-as-errors "${extract_tmp}/kasane-ledger-time-constructors.llbc"
diff -u proofs/extraction/u256-lean/LedgerTimeGenerated.lean "${extract_tmp}/Kasane-ledger-time-constructors.lean"
export RUSTFLAGS='-Coverflow-checks=no'
target/debug/charon cargo --preset=aeneas --mir optimized --error-on-warnings   --start-from kasane_ledger_timestamp_probe::new_time   --dest-file "${extract_tmp}/kasane-ledger-time-wrapping.llbc"   -- --manifest-path proofs/extraction/ledger-time-rust/Cargo.toml --lib --locked
.local/proof-tools/aeneas-source/src/_build/default/main.exe -backend lean   -dest "${extract_tmp}" -namespace LedgerTimeWrapping -use-lean-modules false   -abort-on-error -warnings-as-errors "${extract_tmp}/kasane-ledger-time-wrapping.llbc"
diff -u proofs/extraction/u256-lean/LedgerTimeWrappingGenerated.lean "${extract_tmp}/Kasane-ledger-time-wrapping.lean"
cd proofs/extraction/u256-lean
lake build
lake env lean -DwarningAsError=true LedgerTimeCorrespondence.lean
lake env lean -DwarningAsError=true LedgerTimeWrappingCorrespondence.lean
lake env lean -DwarningAsError=true Audit.lean
lake env leanchecker LedgerTimeCorrespondence
lake env leanchecker LedgerTimeWrappingCorrespondence
echo '[verify-ledger-time-experimental] fixed-release TimeStamp source, checked and wrapping extraction, all-input constructor/accessor proofs, audit and kernel recheck passed'
