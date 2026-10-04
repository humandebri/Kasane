#!/usr/bin/env bash
# Verifies isolated native-layout Nanoseconds read extraction; unsafe constructor stays rejected.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
repo_root="$PWD"
shasum -a 256 -c proofs/extraction/ledger-time-sources.sha256
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export RUSTFLAGS='-Coverflow-checks=yes'
export CARGO_TARGET_DIR="${repo_root}/.local/proof-tools/ledger-timestamp-target"
export DYLD_LIBRARY_PATH="${repo_root}/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
cargo test --manifest-path proofs/extraction/ledger-time-rust/Cargo.toml --locked --test nanoseconds_source
CARGO_TARGET_DIR="${repo_root}/.local/proof-tools/nanoseconds-miri-target" cargo miri test --manifest-path proofs/extraction/ledger-time-rust/Cargo.toml --locked --test nanoseconds_source
python3 - <<'PYTEST'
import hashlib,json,pathlib,subprocess,tempfile
p=json.load(open('proofs/extraction/tool-patches/nanoseconds-read-profile.json'))
fixed=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
def hashes():
    for mapping in [p['sha256'],p['base_source_sha256'],p['rust_primary_sources'],fixed['experimental_binary_sha256']]:
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
            check=subprocess.run(['.local/proof-tools/aeneas-nanoseconds-read/check',str(dest)],capture_output=True,text=True)
            log=check.stdout+check.stderr
            assert check.returncode==0 and '20 borrow/outlive comparisons, 12 strict negatives' in log,log
            assert 'both actual typed transmute bodies retained' in log,log
            assert '24 metadata negatives, reverse direction and wrong scalar rejected' in log,log
            print('actual full Nanoseconds: exact U32 range, both typed cast bodies, 20 classification pairs and 12 strict negatives passed')
        for label,exe in [('fixed','.local/proof-tools/aeneas-source/src/_build/default/main.exe'),
                          ('candidate','.local/proof-tools/aeneas-nanoseconds-read/aeneas')]:
            output=tmp/(mode+'-'+label);output.mkdir()
            result=subprocess.run([exe,'-backend','lean','-abort-on-error','-warnings-as-errors',
                 '-split-files','-use-lean-modules','false','-all-computable','-dest',str(output),str(dest)],
                 capture_output=True,text=True)
            log=result.stdout+result.stderr
            assert result.returncode!=0 and not list(output.rglob('*.lean')),log
            if mode=='opaque' or label=='candidate':
                assert 'Invalid input for unop: transmute<' in log
                assert 'interp/InterpExpressions.ml' in log
                assert 'transmute<u32, core::num::niche_types::Nanoseconds>' in log
                if label=='fixed' or mode=='opaque':
                    assert 'transmute<core::num::niche_types::Nanoseconds, u32>' in log
                else:
                    assert 'Invalid input for unop: transmute<core::num::niche_types::Nanoseconds, u32>' not in log
            elif label=='fixed':
                assert 'unsupported type:' in log and 'TPattern' in log
                assert 'llbc/TypesAnalysis.ml, line 486' in log
            print(mode+' '+label+': original strict unsupported feature rejected; no Lean output accepted')
    getter=tmp/'DurationRead.llbc'
    subprocess.run(['target/debug/charon','cargo','--preset=aeneas','--mir','optimized','--error-on-warnings',
         '--start-from','core::time::_::as_secs','--include','core::time::_::as_secs',
         '--start-from','core::time::_::as_nanos','--include','core::time::_::as_nanos',
         '--include','core::num::niche_types::_::as_inner','--include','core::time::NANOS_PER_SEC',
         '--include','core::time::Duration','--include','core::num::niche_types::Nanoseconds',
         '--dest-file',str(getter),'--','--manifest-path',
         'proofs/extraction/ledger-time-rust/Cargo.toml','--lib','--locked'],check=True)
    data=json.loads(getter.read_text());assert data['has_errors'] is False
    patterns=[];clean(data['translated']);assert len(patterns)==1
    bodies=[d for d in data['translated']['fun_decls'] if d and
            isinstance(d['body'],dict) and 'Structured' in d['body']]
    assert len(bodies)==4,bodies
    names=[[part['Ident'][0] for part in d['item_meta']['name'] if 'Ident' in part] for d in bodies]
    for name in [['core','time','as_secs'],['core','time','as_nanos'],['core','num','niche_types','as_inner']]:assert name in names,names
    output=tmp/'getter';output.mkdir()
    subprocess.run(['.local/proof-tools/aeneas-nanoseconds-read/aeneas','-backend','lean',
         '-abort-on-error','-warnings-as-errors','-use-lean-modules','false','-all-computable',
         '-namespace','DurationRead','-dest',str(output),str(getter)],check=True)
    actual=(output/'DurationRead.lean').read_bytes()
    assert actual==pathlib.Path('proofs/extraction/operator-lean/DurationReadGenerated.lean').read_bytes()
    assert b'999999999' in actual and b'Subtype' not in actual # emitted explicit subtype syntax
    assert b'{ rangeValue : Std.U32 //' in actual
    assert b'axiom' not in actual and b'sorry' not in actual
    # Actual read must keep rejecting a broken layout; no projection fallback.
    import copy
    broken=copy.deepcopy(data)
    nano=next(d for d in broken['translated']['type_decls'] if d and
       [p['Ident'][0] for p in d['item_meta']['name'] if 'Ident' in p]==['core','num','niche_types','Nanoseconds'])
    nano['layout'][0]['value']['repr']['transparent']=False
    bad=tmp/'BrokenLayout.llbc';bad.write_text(json.dumps(broken))
    negative_output=tmp/'broken';negative_output.mkdir()
    result=subprocess.run(['.local/proof-tools/aeneas-nanoseconds-read/aeneas','-backend','lean',
        '-abort-on-error','-warnings-as-errors','-dest',str(negative_output),str(bad)],capture_output=True,text=True)
    log=result.stdout+result.stderr
    assert result.returncode!=0 and not list(negative_output.rglob('*.lean')),log
    assert 'Invalid input for unop: transmute<core::num::niche_types::Nanoseconds, u32>' in log,log
    print('actual Nanoseconds read and Duration getters regenerated unchanged; broken layout strictly rejected')
hashes()
PYTEST
cd proofs/extraction/operator-lean
lake build DurationReadAudit
for file in DurationReadGenerated.lean DurationReadCorrespondence.lean DurationReadRefinement.lean DurationReadAudit.lean; do
    lake env lean -DwarningAsError=true "$file"
done
lake env leanchecker DurationReadCorrespondence
lake env leanchecker DurationReadRefinement
python3 - <<'PYTEST'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('DurationReadAudit.lean').read_text().split('open Lean in',1)
mutations={
 'axiom':'axiom bad : False\ntheorem negative : False := bad',
 'sorry':'theorem negative : False := by sorry',
 'native':'theorem negative : (1 : Nat) = 1 := by native_decide'}
with tempfile.TemporaryDirectory(prefix='duration-negative-',dir='.lake') as tmp:
    for name,mutation in mutations.items():
        f=pathlib.Path(tmp)/f'{name}.lean'
        f.write_text(head+'import Std.Tactic\nnamespace DurationReadCorrespondence\n'+mutation+'\nend DurationReadCorrespondence\nopen Lean in'+body)
        result=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
        assert result.returncode!=0 and 'depends on forbidden axiom' in result.stdout+result.stderr,result.stdout+result.stderr
        print(name+' rejected by range audit')
PYTEST
echo '[verify-nanoseconds-read-candidate] actual range and getter regenerated; 5 source plus 6 refinement theorems, 24 audited declarations, kernel recheck and 3 forbidden-axiom negatives passed; reverse constructor, translator semantics and Wasm correspondence remain unproved'
