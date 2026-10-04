#!/usr/bin/env bash
# Isolated unadopted frontend candidate; does not prove GAT/EVM correspondence.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/gat-candidate-check-target"
python3 - <<'PYTEST'
import hashlib,json,pathlib,subprocess,tempfile
profile=json.load(open('proofs/extraction/tool-patches/gat-closed-item-equality-profile.json'))
fixed=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
for filename,expected in {**profile['sha256'],**fixed['experimental_binary_sha256']}.items():
    assert hashlib.sha256(pathlib.Path(filename).read_bytes()).hexdigest()==expected,filename
exe='.local/proof-tools/charon-gat-candidate/charon'
def extract(manifest,case,dest):
    args=['--manifest-path',manifest,'--lib','--features',case,'--locked']
    check=subprocess.run(['cargo','check',*args],capture_output=True,text=True)
    assert check.returncode==0,check.stdout+check.stderr
    r=subprocess.run([exe,'cargo','--preset=aeneas','--mir','promoted',
      '--error-on-warnings','--dest-file',str(dest),'--',*args],capture_output=True,text=True)
    return r,json.loads(dest.read_text())
def assert_clean(data):
    assert data['has_errors'] is False
    def walk(x):
        if isinstance(x,dict):
            assert not x.get('has_errors',False),x
            assert not ('Error' in x),x
            for c in x.values():walk(c)
        elif isinstance(x,list):
            for c in x:walk(c)
    walk(data['translated'])
def check_mixed(data):
    types={}
    def collect(x):
        if isinstance(x,dict):
            if 'Value' in x:
                i,p=x['Value']
                if isinstance(p,dict) and set(p)&{'Slice','Scalar','Adt','Ref','TraitType','Var'}:
                    types[i]=p
            for c in x.values():collect(c)
        elif isinstance(x,list):
            for c in x:collect(c)
    collect(data)
    def resolve(t):
        return types[t['Deduplicated']] if 'Deduplicated' in t else t['Value'][1]
    trait=next(t for t in data['translated']['trait_decls'] if t and
      'MemoryTr' in str(t['item_meta']['name']))
    observed={}
    for assoc in trait['types']:
        assert len(assoc['params']['regions'])==1
        clause=assoc['skip_binder']['implied_clauses'][0]
        target=clause['trait_']['skip_binder']['generics']['types'][1]
        scalar=resolve(resolve(target)['Slice'][0])
        observed[assoc['skip_binder']['name']]=scalar['Scalar']['Integer']['Unsigned']
    assert observed=={'Bytes':'U8','Words':'U16'},observed
with tempfile.TemporaryDirectory(prefix='kasane-gat-candidate-') as tmp:
    for case in ['plain','rpit','named','mixed','dependent']:
        manifest=f"proofs/extraction/gat-{'boundary' if case in ['mixed','dependent'] else 'diagnostic'}-rust/Cargo.toml"
        result,data=extract(manifest,case,pathlib.Path(tmp)/f'{case}.llbc')
        if case=='dependent':
            assert result.returncode!=0 and data['has_errors'] is True
            assert 'GATs cannot work' in result.stderr+result.stdout
            print('dependent type equality: rejected; no unsupported substitution accepted')
        else:
            assert result.returncode==0,result.stderr+result.stdout
            assert_clean(data)
            if case=='mixed':check_mixed(data)
            print(f'{case}: clean extraction; mixed GAT targets and lifetime binders checked' if case=='mixed' else f'{case}: clean extraction')
    actual=json.load(open('proofs/extraction/tool-patches/fixtures/revm-gat-closed-candidate.json'))
    assert_clean(actual)
    memory=next(t for t in actual['translated']['trait_decls'] if t and 'MemoryTr' in str(t['item_meta']['name']))
    assert len(memory['types'])==1 and len(memory['types'][0]['params']['regions'])==1
    print('fixed actual ADD/SUB candidate fixture: no error nodes; MemoryTr GAT/lifetime binder retained (fixture validation only)')
print('[verify-gat-closed-candidate] closed-equality frontend checks passed; fixed tool unchanged; candidate correctness, full GAT/Lean/EVM correspondence unproved')
PYTEST
