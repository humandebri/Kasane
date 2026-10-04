#!/usr/bin/env bash
# Actual unwrap, error and Nano source; local Debug/unwrap_failed observations remain modeled.
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
profile=json.load(open('proofs/extraction/ledger-actual-unwrap-profile.json'))
fixed=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
candidate=json.load(open('proofs/extraction/tool-patches/never-call-profile.json'))
def hashes():
    for mapping in [profile['sha256'],profile['rust_primary_sources'],fixed['experimental_binary_sha256'],candidate['sha256'],candidate['base_source_sha256']]:
        for f,h in mapping.items(): assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
hashes()
subprocess.run(['cargo','test','--manifest-path','proofs/extraction/ledger-time-rust/Cargo.toml','--locked','--test','duration','--test','duration_source','--test','nanoseconds_source','--test','int_error_source','--test','unwrap_source'],check=True)
subprocess.run(['cargo','miri','test','--manifest-path','proofs/extraction/ledger-time-rust/Cargo.toml','--locked','--test','nanoseconds_source','--test','int_error_source','--test','unwrap_source'],check=True)
with tempfile.TemporaryDirectory(prefix='kasane-ledger-duration-') as tmp:
    tmp=pathlib.Path(tmp);dest=tmp/'LedgerActualUnwrap.llbc'
    cmd=['target/debug/charon','cargo','--preset=aeneas','--mir','optimized','--error-on-warnings',
         '--start-from','kasane_ledger_timestamp_probe::add_time',
         '--start-from','kasane_ledger_timestamp_probe::sub_time',
         '--include','core::time::Duration','--include','core::num::niche_types::Nanoseconds',
         '--include','core::num::niche_types::_::new_unchecked',
         '--include','core::num::niche_types::_::as_inner',
         '--include','core::time::_::from_nanos','--include','core::time::_::as_nanos',
         '--include','core::time::NANOS_PER_SEC','--include','core::convert::num::_::try_from',
         '--include','core::num::error::TryFromIntError',
         '--include','core::result::_::unwrap','--opaque','core::result::unwrap_failed',
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
    structured=[d for d in data['translated']['fun_decls'] if d and
                isinstance(d['body'],dict) and 'Structured' in d['body']]
    globals_=[d for d in structured if isinstance(d['src'],dict) and 'GlobalInitializer' in d['src']]
    assert len(structured)==13 and len(globals_)==2
    source_names=[[part['Ident'][0] for part in d['item_meta']['name'] if 'Ident' in part]
                  for d in structured]
    assert ['core','time','from_nanos'] in source_names and ['core','time','as_nanos'] in source_names
    types={tuple(part['Ident'][0] for part in d['item_meta']['name'] if 'Ident' in part):d
           for d in data['translated']['type_decls'] if d}
    assert len(types[('core','time','Duration')]['kind']['Struct'])==2
    assert 'Struct' in types[('core','num','niche_types','Nanoseconds')]['kind']
    check=subprocess.run(['.local/proof-tools/aeneas-never-call/check',str(dest)],capture_output=True,text=True)
    log=check.stdout+check.stderr
    assert check.returncode==0 and 'read and construct guards passed; 24 metadata negatives' in log,log
    assert '13 malformed definitions rejected; Unit builtin removed; all other Lean builtin mappings unchanged' in log,log
    assert 'inhabited empty match rejected; only unwrap builtin removed' in log,log
    assert '16 string emissions preserve every source byte' in log,log
    # Error constructors are intentionally retained, not parsed from an errored extraction.
    assert 'PosOverflow' in dest.read_text()
    (tmp/'LedgerActualUnwrap').mkdir()
    subprocess.run(['.local/proof-tools/aeneas-never-call/aeneas',
         '-backend','lean','-abort-on-error','-warnings-as-errors','-split-files',
         '-use-lean-modules','false','-all-computable','-namespace','LedgerActualUnwrap',
         '-dest',str(tmp/'LedgerActualUnwrap'),str(dest)],check=True)
    for f in ['Types.lean','Funs.lean']:
        expected=pathlib.Path('proofs/extraction/operator-lean/LedgerActualUnwrap')/f
        assert expected.read_bytes()==(tmp/'LedgerActualUnwrap'/f).read_bytes(),f
    emitted_types=(tmp/'LedgerActualUnwrap'/'Types.lean').read_text()
    assert 'def core.num.error.TryFromIntError := core.num.error.IntErrorKind' in emitted_types
    assert 'rust_type "core::num::error::TryFromIntError"' in emitted_types
    assert 'def core.result.Result.unwrap' in (tmp/'LedgerActualUnwrap'/'Funs.lean').read_text()
    generated=(tmp/'LedgerActualUnwrap'/'Funs.lean').read_text()
    assert 'nomatch x' in generated and 'core.result.unwrap_failed' in generated
    assert 'Slice.from [' in generated and '(by scalar_tac)' in generated and 'toStr' not in generated
    unwrap=next(f for f in structured if [p['Ident'][0] for p in f['item_meta']['name'] if 'Ident' in p]==['core','result','unwrap'])
    body=unwrap['body']['Structured']['body']
    switch=next(s['kind']['Switch'] for s in body['statements'] if isinstance(s['kind'],dict) and 'Switch' in s['kind'])
    assert len(switch['branches'])==2
    assert [len(b['statements']) for b in switch['branches']]==[0,15]
    call=switch['branches'][1]['statements'][-1]['kind']['Call']
    failed=next(f for f in data['translated']['fun_decls'] if f and [p['Ident'][0] for p in f['item_meta']['name'] if 'Ident' in p]==['core','result','unwrap_failed'])
    assert call['call']['func']['Regular']['kind']['Fun']==failed['def_id']
    never=unwrap['body']['Structured']['locals']['locals'][4]['ty']
    assert never['Value'][1]=='Never' and call['call']['dest']['ty']=={'Deduplicated':never['Value'][0]}
    assert call['on_unwind'] and 'Drop' in json.dumps(call['on_unwind']) and 'UnwindResume' in json.dumps(call['on_unwind'])
    preceding=tmp/'preceding';preceding.mkdir()
    result=subprocess.run(['.local/proof-tools/aeneas-int-error-source/aeneas',
       '-backend','lean','-abort-on-error','-warnings-as-errors','-dest',str(preceding),str(dest)],capture_output=True,text=True)
    log=result.stdout+result.stderr
    assert result.returncode!=0 and 'InterpPaths.ml' in log and not list(preceding.rglob('*.lean')),log
    print('preceding candidate reproduces erroneous Never fallthrough; source unwrap has both variants, exact call and on_unwind retained')
    import copy
    for mode in ['opaque-error','changed-payload']:
        invalid=copy.deepcopy(data)
        types_={tuple(p['Ident'][0] for p in d['item_meta']['name'] if 'Ident' in p):d
                for d in invalid['translated']['type_decls'] if d}
        if mode=='opaque-error':
            error=types_[('core','num','error','TryFromIntError')]
            payload_ty=error['kind']['Struct'][0]['ty']
            missing_id=payload_ty['Value'][0]
            # Keep the hash-consed payload definition alive after removing its field.
            def restore_payload_definition(x):
                if isinstance(x,dict):
                    if x.get('SizeOf')=={'Deduplicated':missing_id}:
                        x['SizeOf']=payload_ty
                        return True
                    return any(restore_payload_definition(v) for v in x.values())
                if isinstance(x,list):return any(restore_payload_definition(v) for v in x)
                return False
            assert restore_payload_definition(error['layout'])
            error['kind']='Opaque'

        else:types_[('core','num','error','IntErrorKind')]['kind']['Enum'][0]['name']='Other'
        input_file=tmp/(mode+'.llbc');input_file.write_text(json.dumps(invalid))
        output=tmp/mode;output.mkdir()
        result=subprocess.run(['.local/proof-tools/aeneas-never-call/aeneas',
            '-backend','lean','-abort-on-error','-warnings-as-errors','-dest',str(output),
            str(input_file)],capture_output=True,text=True)
        log=result.stdout+result.stderr
        assert result.returncode!=0 and 'Unsupported TryFromIntError source definition' in log,log
        assert not list(output.rglob('*.lean')),log
        print(mode+': changed source definition rejected, no Unit fallback')

    # Keep fixed/older candidate strict failures and broken-layout rejection.
    for label,exe,broken in [
        ('fixed','.local/proof-tools/aeneas-source/src/_build/default/main.exe',False),
        ('read-only','.local/proof-tools/aeneas-nanoseconds-read/aeneas',False),
        ('broken-layout','.local/proof-tools/aeneas-never-call/aeneas',True)]:
        input_file=dest
        if broken:
            import copy
            invalid=copy.deepcopy(data)
            nano=next(d for d in invalid['translated']['type_decls'] if d and
              [p['Ident'][0] for p in d['item_meta']['name'] if 'Ident' in p]==['core','num','niche_types','Nanoseconds'])
            nano['layout'][0]['value']['repr']['transparent']=False
            input_file=tmp/'BrokenLayout.llbc';input_file.write_text(json.dumps(invalid))
        output=tmp/label;output.mkdir()
        negative=subprocess.run([exe,'-backend','lean','-abort-on-error',
            '-warnings-as-errors','-dest',str(output),str(input_file)],capture_output=True,text=True)
        log=negative.stdout+negative.stderr
        assert negative.returncode!=0 and not list(output.rglob('*.lean')),log
        if label=='fixed':assert 'unsupported type' in log and 'TPattern' in log,log
        else:assert 'Invalid input for unop: transmute<u32, core::num::niche_types::Nanoseconds>' in log,log
        print(label+': strict failure retained, no partial Lean output accepted')

hashes()
PYTEST
cd proofs/extraction/operator-lean
lake build LedgerActualUnwrapAudit
for file in LedgerActualUnwrap/Types.lean LedgerActualUnwrap/Funs.lean LedgerActualUnwrap/FunsExternal.lean LedgerActualUnwrapCorrespondence.lean LedgerActualUnwrapRefinement.lean LedgerDurationModel.lean NeverCallCorrespondence.lean LedgerActualUnwrapAudit.lean; do
    lake env lean -DwarningAsError=true "$file"
done
lake env leanchecker LedgerActualUnwrapCorrespondence
lake env leanchecker LedgerActualUnwrapRefinement
lake env leanchecker NeverCallCorrespondence
python3 - <<'PYTEST'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('LedgerActualUnwrapAudit.lean').read_text().split('open Lean in',1)
mutations={
 'axiom':'axiom bad : False\ntheorem negative : False := bad',
 'sorry':'theorem negative : False := by sorry',
 'native':'theorem negative : (1 : Nat) = 1 := by native_decide'}
with tempfile.TemporaryDirectory(prefix='duration-negative-',dir='.lake') as tmp:
    for name,mutation in mutations.items():
        f=pathlib.Path(tmp)/f'{name}.lean'
        f.write_text(head+'import Std.Tactic\nnamespace LedgerActualUnwrapCorrespondence\n'+mutation+'\nend LedgerActualUnwrapCorrespondence\nopen Lean in'+body)
        result=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
        assert result.returncode!=0 and 'depends on forbidden axiom' in result.stdout+result.stderr,result.stdout+result.stderr
        print(name+' rejected by duration audit')
PYTEST
echo '[verify-ledger-actual-unwrap] unchanged 13-body extraction including source unwrap; 19 conditional Duration, 5 refinement and 6 Never/source unwrap theorems; strict audit/kernel and negative controls passed; helper formatting/unwind and translator/runtime/IC/ledger refinement remain unproved'
