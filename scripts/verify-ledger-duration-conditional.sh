#!/usr/bin/env bash
# Actual source bodies with explicit Duration/Debug/unwrap models; not full Rust correspondence.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
repo_root="$PWD"
shasum -a 256 -c proofs/extraction/ledger-time-sources.sha256
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export RUSTFLAGS='-Coverflow-checks=yes'
export CARGO_TARGET_DIR="${repo_root}/.local/proof-tools/ledger-timestamp-target"
export DYLD_LIBRARY_PATH="${repo_root}/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
python3 - <<'PYTEST'
import hashlib,json,pathlib,subprocess,tempfile
profile=json.load(open('proofs/extraction/ledger-duration-profile.json'))
fixed=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
def hashes():
    for mapping in [profile['sha256'],profile['rust_primary_sources'],fixed['experimental_binary_sha256']]:
        for f,h in mapping.items(): assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
hashes()
subprocess.run(['cargo','test','--manifest-path','proofs/extraction/ledger-time-rust/Cargo.toml','--locked','--test','duration'],check=True)
with tempfile.TemporaryDirectory(prefix='kasane-ledger-duration-') as tmp:
    tmp=pathlib.Path(tmp);dest=tmp/'LedgerDurationExtract.llbc'
    cmd=['target/debug/charon','cargo','--preset=aeneas','--mir','optimized','--error-on-warnings',
         '--start-from','kasane_ledger_timestamp_probe::add_time',
         '--start-from','kasane_ledger_timestamp_probe::sub_time',
         '--opaque','core::time::Duration','--include','core::convert::num::_::try_from',
         '--include','core::num::error::TryFromIntError',
         '--dest-file',str(dest),'--','--manifest-path','proofs/extraction/ledger-time-rust/Cargo.toml','--lib','--locked']
    subprocess.run(cmd,check=True)
    data=json.loads(dest.read_text());assert data['has_errors'] is False
    def clean(x):
        if isinstance(x,dict):
            assert not x.get('has_errors',False) and 'Error' not in x,x
            for v in x.values():clean(v)
        elif isinstance(x,list):
            for v in x:clean(v)
    clean(data['translated'])
    # Error constructors are intentionally retained, not parsed from an errored extraction.
    assert 'PosOverflow' in dest.read_text()
    (tmp/'LedgerDurationExtract').mkdir()
    subprocess.run(['.local/proof-tools/aeneas-source/src/_build/default/main.exe',
         '-backend','lean','-abort-on-error','-warnings-as-errors','-split-files',
         '-use-lean-modules','false','-all-computable','-namespace','LedgerDuration',
         '-dest',str(tmp/'LedgerDurationExtract'),str(dest)],check=True)
    for f in ['Types.lean','Funs.lean']:
        expected=pathlib.Path('proofs/extraction/operator-lean/LedgerDurationExtract')/f
        assert expected.read_bytes()==(tmp/'LedgerDurationExtract'/f).read_bytes(),f
    # Keep the unsupported full std Duration extraction as a strict negative control.
    full=tmp/'DurationFull.llbc'
    fullcmd=cmd.copy()
    fullcmd[fullcmd.index('--opaque')]='--include'
    fullcmd[fullcmd.index(str(dest))]=str(full)
    insert=fullcmd.index('--dest-file')
    fullcmd[insert:insert]=['--include','core::num::niche_types::Nanoseconds']
    subprocess.run(fullcmd,check=True)
    full_data=json.loads(full.read_text());assert full_data['has_errors'] is False
    clean(full_data['translated'])
    assert '"Pattern"' in full.read_text()
    negative=subprocess.run(['.local/proof-tools/aeneas-source/src/_build/default/main.exe',
        '-backend','lean','-abort-on-error','-warnings-as-errors','-dest',str(tmp),str(full)],
        capture_output=True,text=True)
    log=negative.stdout+negative.stderr
    assert negative.returncode!=0 and 'unsupported type' in log and 'TPattern' in log,log
    print('full std Duration still rejects range-typed TPattern; no constraint erasure accepted')
hashes()
PYTEST
cd proofs/extraction/operator-lean
lake build LedgerDurationAudit
for file in LedgerDurationExtract/TypesExternal.lean LedgerDurationExtract/FunsExternal.lean LedgerDurationCorrespondence.lean LedgerDurationAudit.lean; do
    lake env lean -DwarningAsError=true "$file"
done
lake env leanchecker LedgerDurationCorrespondence
python3 - <<'PYTEST'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('LedgerDurationAudit.lean').read_text().split('open Lean in',1)
mutations={
 'axiom':'axiom bad : False\ntheorem negative : False := bad',
 'sorry':'theorem negative : False := by sorry',
 'native':'theorem negative : (1 : Nat) = 1 := by native_decide'}
with tempfile.TemporaryDirectory(prefix='duration-negative-',dir='.lake') as tmp:
    for name,mutation in mutations.items():
        f=pathlib.Path(tmp)/f'{name}.lean'
        f.write_text(head+'import Std.Tactic\nnamespace LedgerDurationCorrespondence\n'+mutation+'\nend LedgerDurationCorrespondence\nopen Lean in'+body)
        result=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
        assert result.returncode!=0 and 'depends on forbidden axiom' in result.stdout+result.stderr,result.stdout+result.stderr
        print(name+' rejected by duration audit')
PYTEST
echo '[verify-ledger-duration-conditional] unchanged extraction, 15 explicit model theorems, axiom audit, kernel recheck and negative cases passed; Duration/formatting/runtime refinement remains unproved'
