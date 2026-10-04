#!/usr/bin/env bash
# Unadopted declaration-return candidate; preserve implementation differences.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/gat-override-target"
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
python3 - <<'PYTEST'
import hashlib,json,pathlib,subprocess,tempfile
profiles=[json.load(open('proofs/extraction/tool-patches/'+name)) for name in
          ['rpit-signature-profile.json','gat-closed-item-equality-profile.json','revm-add-sub-borrow-profile.json']]
fixed=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
def check_hashes():
    for p in profiles:
        for f,h in p['sha256'].items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
    for f,h in fixed['experimental_binary_sha256'].items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
check_hashes()
def clean(data):
    assert data['has_errors'] is False
    def walk(x):
        if isinstance(x,dict):
            assert not x.get('has_errors',False) and 'Error' not in x,x
            for c in x.values():walk(c)
        elif isinstance(x,list):
            for c in x:walk(c)
    walk(data['translated'])
def name(item):return '::'.join(p['Ident'][0] for p in item['item_meta']['name'] if 'Ident' in p)
def inspect_override(data,is_candidate):
    clean(data)
    values={};refs={}
    def collect(x):
        if isinstance(x,dict):
            if 'Value' in x:
                i,p=x['Value']
                if isinstance(p,dict):
                    if set(p)&{'Adt','Ref','Slice','Scalar','TraitType','TypeVar','Array'}:values[i]=p
                    if 'trait_decl_ref' in p:refs[i]=p
            for c in x.values():collect(c)
        elif isinstance(x,list):
            for c in x:collect(c)
    collect(data)
    def resolve(t):return values[t['Deduplicated']] if 'Deduplicated' in t else t['Value'][1]
    def resolve_ref(t):return refs[t['Deduplicated']] if 'Deduplicated' in t else t['Value'][1]
    types={t['def_id']:name(t) for t in data['translated']['type_decls'] if t}
    trait=next(t for t in data['translated']['trait_decls'] if t and name(t)=='kasane_gat_override::MemoryTr')
    method=next(m['skip_binder'] for m in trait['methods'] if m['skip_binder']['name']=='slice_len')
    assoc=trait['types'][0]
    assert len(assoc['params']['regions'])==1 and assoc['params']['types_outlive'] and assoc['params']['trait_type_constraints']
    assert assoc['skip_binder']['implied_clauses']
    def assert_borrowed_u8(t):
        region,target,kind=resolve(t)['Ref'];assert kind=='Shared'
        element=resolve(resolve(target)['Slice'][0])
        assert element=={'Scalar':{'Integer':{'Unsigned':'U8'}}},element
    assert_borrowed_u8(assoc['skip_binder']['default']['value'])
    default=next(f for f in data['translated']['fun_decls'] if f and name(f)=='kasane_gat_override::MemoryTr::slice_len')
    assert 'TraitDefault' in default['src'];assert_borrowed_u8(default['signature']['output'])
    output=resolve(method['signature']['output'])
    if is_candidate:
        tref,type_id,args=output['TraitType']
        tref=resolve_ref(tref)
        assert tref['kind']=='SelfId' and tref['trait_decl_ref']['skip_binder']['id']==trait['def_id'] and type_id==0
        assert args==dict(regions=[{'Var':{'Bound':[0,0]}}],types=[],const_generics=[],trait_refs=[])
    else:assert_borrowed_u8(method['signature']['output'])
    implementations={}
    for impl in data['translated']['trait_impls']:
        if impl and impl['impl_trait']['id']==trait['def_id']:
            self_ty=resolve(impl['impl_trait']['generics']['types'][0])['Adt']
            implementations[types[self_ty['id']]]=impl['types'][0]['skip_binder']['value']
    assert set(implementations)=={'kasane_gat_override::DefaultMemory','kasane_gat_override::OverrideMemory'}
    assert_borrowed_u8(implementations['kasane_gat_override::DefaultMemory'])
    owned=resolve(implementations['kasane_gat_override::OverrideMemory'])['Adt']
    assert types[owned['id']]=='alloc::vec::Vec'
    assert resolve(owned['generics']['types'][0])=={'Scalar':{'Integer':{'Unsigned':'U8'}}}
with tempfile.TemporaryDirectory(prefix='kasane-rpit-signature-') as tmp:
    tmp=pathlib.Path(tmp)
    def run(cmd,logfile):
        with logfile.open('w') as out:r=subprocess.run(cmd,stdout=out,stderr=subprocess.STDOUT,timeout=180)
        return r.returncode,logfile.read_text()
    for case in ['plain','rpit','named','mixed','dependent']:
        manifest=f"proofs/extraction/gat-{'boundary' if case in ['mixed','dependent'] else 'diagnostic'}-rust/Cargo.toml"
        dest=tmp/f'{case}.llbc'
        rc,log=run(['.local/proof-tools/charon-rpit-signature-candidate/charon','cargo','--preset=aeneas',
            '--mir','promoted','--error-on-warnings','--dest-file',str(dest),'--',
            '--manifest-path',manifest,'--lib','--features',case,'--locked'],tmp/f'{case}.log')
        data=json.loads(dest.read_text())
        if case=='dependent':
            assert rc!=0 and data['has_errors'] is True and 'GATs cannot work' in log
            dest.unlink()
            print('parameter-dependent GAT equality: still strictly rejected; invalid output not consumed')
        else:
            assert rc==0,log
            clean(data)
            print(case+': candidate clean extraction')
    manifest='proofs/extraction/gat-override-rust/Cargo.toml'
    rc,log=run(['cargo','test','--manifest-path',manifest,'--lib','--locked'],tmp/'native.log')
    assert rc==0 and '2 passed; 0 failed' in log,log
    for candidate in [False,True]:
        directory='charon-rpit-signature-candidate' if candidate else 'charon-gat-candidate'
        dest=tmp/('new.llbc' if candidate else 'old.llbc')
        rc,log=run([f'.local/proof-tools/{directory}/charon','cargo','--preset=aeneas','--mir','promoted',
            '--monomorphize-mut=all','--error-on-warnings','--dest-file',str(dest),'--',
            '--manifest-path',manifest,'--lib','--locked'],tmp/('new.log' if candidate else 'old.log'))
        assert rc==0,log
        inspect_override(json.loads(dest.read_text()),candidate)
        print(('candidate GAT declaration preserved' if candidate else 'old concrete declaration reproduced')+'; default borrowed slice and override owned Vec retained')
    rc,log=run(['.local/proof-tools/aeneas-ref-copy-candidate/aeneas','-borrow-check','-abort-on-error',
        '-warnings-as-errors',str(tmp/'new.llbc')],tmp/'borrow.log')
    assert rc!=0 and 'deref.rs' in log and 'InterpBorrowsCore.ml, line 476' in log,log
    assert 'InterpPaths.ml, line 260' not in log
    print('prior call type mismatch passed; GAT/Deref lifetime comparison remains unsupported')
check_hashes()
print('[verify-rpit-signature-candidate] native/typed-override checks passed; candidate semantics and full instruction correctness unproved')
PYTEST
bash scripts/diagnose-revm-instructions.sh rpit-signature
