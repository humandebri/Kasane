#!/usr/bin/env bash
# Representation classifier only; reject extraction until memory correspondence is verified.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/ledger-timestamp-target"
export RUSTFLAGS='-Coverflow-checks=yes'
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
python3 - <<'PYGATE'
import hashlib,json,pathlib,subprocess,tempfile
p=json.load(open('proofs/extraction/tool-patches/array-nonnull-profile.json'))
def hashes():
 for key in ['base_source_sha256','sha256']:
  for f,h in p[key].items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
hashes()
cmd=['target/debug/charon','cargo','--lift-associated-types','*']
for flag in ['treat-box-as-builtin','ops-to-function-calls','index-to-function-calls','reconstruct-fallible-operations','reconstruct-asserts','reconstruct-matches','hide-marker-traits','hide-allocator','remove-unused-self-clauses','remove-adt-clauses','unbind-item-vars','deallocate-all-locals','no-gen-tuple-structs']:
 cmd+=['--'+flag]
cmd+=['--mir','optimized','--error-on-warnings','--start-from','kasane_ledger_timestamp_probe::add_time','--start-from','kasane_ledger_timestamp_probe::sub_time']
for item in ['core::time::Duration','core::num::niche_types::Nanoseconds','core::num::niche_types::_::new_unchecked','core::num::niche_types::_::as_inner','core::time::_::from_nanos','core::time::_::as_nanos','core::time::NANOS_PER_SEC','core::convert::num::_::try_from','core::num::error::TryFromIntError','core::result::_::unwrap','core::result::unwrap_failed','core::fmt::rt::Argument','core::fmt::rt::ArgumentType','core::fmt::Arguments','core::fmt::rt::_::new_debug','core::fmt::rt::_::new_display','core::fmt::_::new']:
 cmd+=['--include',item]
cmd+=['--include','core::ptr::non_null::NonNull']

with tempfile.TemporaryDirectory(prefix='array-nonnull-source-') as tmp:
 tmp=pathlib.Path(tmp);dest=tmp/'LedgerArrayNonNullSource.llbc'
 subprocess.run(cmd+['--opaque','core::panicking::panic_fmt','--dest-file',str(dest),'--','--manifest-path','proofs/extraction/ledger-time-rust/Cargo.toml','--lib','--locked'],check=True)
 d=json.loads(dest.read_text());assert d['has_errors'] is False
 r=subprocess.run(['.local/proof-tools/aeneas-array-nonnull/check',str(dest)],capture_output=True,text=True)
 log=r.stdout+r.stderr
 assert r.returncode==0 and '46 type/declaration/layout controls rejected' in log and 'two actual array-reference to NonNull casts matched fixed transparent layout' in log,log
 print(r.stdout)
 out=tmp/'out';out.mkdir()
 args=['-backend','lean','-abort-on-error','-warnings-as-errors','-split-files','-use-lean-modules','false','-all-computable','-dest',str(out)]
 r=subprocess.run(['.local/proof-tools/aeneas-array-nonnull/aeneas']+args+[str(dest)],capture_output=True,text=True)
 log=r.stdout+r.stderr
 assert r.returncode!=0 and '16/17' in log and 'Array-reference to NonNull requires verified pointer region/provenance correspondence' in log and 'fmt/mod.rs' in log,log
 assert 'Raised at Aeneas__Errors.craise_opt_span' in log and 'Internal error' not in log and 'Unreachable' not in log and "ty_regions shouldn't be called" not in log,log
 assert not list(out.rglob('*.lean'))
 print('Fresh actual source: representation guard reached; explicit region/provenance obligation blocks extraction; zero Lean outputs')
for tool in [['cargo'],['cargo','miri']]:
 subprocess.run(tool+['test','--manifest-path','proofs/extraction/ledger-time-rust/Cargo.toml','--locked','--test','array_nonnull_source'],check=True)
hashes()
PYGATE
