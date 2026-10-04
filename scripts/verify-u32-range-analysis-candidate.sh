#!/usr/bin/env bash
# Verifies an unadopted classification candidate; rejects unsupported Pure types and casts.
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
p=json.load(open('proofs/extraction/tool-patches/u32-range-analysis-profile.json'))
fixed=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
def hashes():
    for mapping in [p['sha256'],p['rust_primary_sources'],fixed['experimental_binary_sha256']]:
        for f,h in mapping.items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
hashes()
with tempfile.TemporaryDirectory(prefix='kasane-u32-range-') as tmp:
    tmp=pathlib.Path(tmp)
    for mode in ['opaque','full']:
        dest=tmp/('LedgerNanoseconds'+mode.title()+'.llbc')
        cmd=['target/debug/charon','cargo','--preset=aeneas','--mir','optimized','--error-on-warnings',
             '--start-from','kasane_ledger_timestamp_probe::add_time',
             '--start-from','kasane_ledger_timestamp_probe::sub_time',
             '--include','core::time::Duration',
             '--opaque' if mode=='opaque' else '--include','core::num::niche_types::Nanoseconds',
             '--include','core::time::_::from_nanos','--include','core::time::_::as_nanos',
             '--include','core::time::NANOS_PER_SEC',
             '--include','core::num::niche_types::_::new_unchecked',
             '--include','core::num::niche_types::_::as_inner',
             '--include','core::convert::num::_::try_from',
             '--include','core::num::error::TryFromIntError','--dest-file',str(dest),'--',
             '--manifest-path','proofs/extraction/ledger-time-rust/Cargo.toml','--lib','--locked']
        subprocess.run(cmd,check=True)
        data=json.loads(dest.read_text());assert data['has_errors'] is False
        patterns=[]
        def clean(x):
            if isinstance(x,dict):
                assert not x.get('has_errors',False) and 'Error' not in x,x
                if 'Pattern' in x:patterns.append(x['Pattern'])
                for v in x.values():clean(v)
            elif isinstance(x,list):
                for v in x:clean(v)
        clean(data['translated'])
        structured=[d for d in data['translated']['fun_decls'] if d and
                    isinstance(d['body'],dict) and 'Structured' in d['body']]
        assert len(structured)==12
        assert len(patterns)==(1 if mode=='full' else 0)
        if mode=='full':
            check=subprocess.run(['.local/proof-tools/aeneas-u32-range-analysis/check',str(dest)],capture_output=True,text=True)
            log=check.stdout+check.stderr
            assert check.returncode==0 and '20 borrow/outlive comparisons, 12 strict negatives' in log,log
            assert 'both actual typed transmute bodies retained' in log,log
            print('actual full Nanoseconds: exact U32 range, both typed cast bodies, 20 classification pairs and 12 strict negatives passed')
        for label,exe in [('fixed','.local/proof-tools/aeneas-source/src/_build/default/main.exe'),
                          ('candidate','.local/proof-tools/aeneas-u32-range-analysis/aeneas')]:
            output=tmp/(mode+'-'+label);output.mkdir()
            result=subprocess.run([exe,'-backend','lean','-abort-on-error','-warnings-as-errors',
                 '-split-files','-use-lean-modules','false','-all-computable','-dest',str(output),str(dest)],
                 capture_output=True,text=True)
            log=result.stdout+result.stderr
            assert result.returncode!=0 and not list(output.rglob('*.lean')),log
            if mode=='opaque':
                assert 'Invalid input for unop: transmute<' in log
                assert 'interp/InterpExpressions.ml, line 983' in log
                assert 'transmute<u32, core::num::niche_types::Nanoseconds>' in log
                assert 'transmute<core::num::niche_types::Nanoseconds, u32>' in log
            elif label=='fixed':
                assert 'unsupported type:' in log and 'TPattern' in log
                assert 'llbc/TypesAnalysis.ml, line 486' in log
            else:
                assert 'unsupported type:' in log and 'TPattern' in log
                assert 'symbolic/SymbolicToPureTypes.ml, line 195' in log
                assert 'llbc/TypesAnalysis.ml, line 486' not in log
            print(mode+' '+label+': original strict unsupported feature rejected; no Lean output accepted')
hashes()
PYTEST
echo '[verify-u32-range-analysis-candidate] exact source casts/range retained, classification guard tests and original/candidate strict failures verified; no unsafe-transmute or full Rust/EVM/IC correspondence theorem'
