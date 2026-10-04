#!/usr/bin/env bash
# Diagnostic only: unapplied frontend/ref-copy candidates, opaque calls retained.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
python3 - <<'PYTEST'
import hashlib,json,pathlib,subprocess,tempfile
profile=json.load(open('proofs/extraction/tool-patches/revm-add-sub-borrow-profile.json'))
fixed=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
frontend=json.load(open('proofs/extraction/tool-patches/gat-closed-item-equality-profile.json'))
def check_hashes():
    for filename,expected in {**frontend['sha256'],**profile['sha256'],**fixed['experimental_binary_sha256']}.items():
        assert hashlib.sha256(pathlib.Path(filename).read_bytes()).hexdigest()==expected,filename
check_hashes()
data=json.load(open('proofs/extraction/tool-patches/fixtures/revm-gat-closed-candidate.json'))
assert data['has_errors'] is False
def clean(x):
    if isinstance(x,dict):
        assert not x.get('has_errors',False) and 'Error' not in x,x
        for c in x.values():clean(c)
    elif isinstance(x,list):
        for c in x:clean(c)
clean(data['translated'])
def function_name(f):
    return '::'.join(part['Ident'][0] for part in f['item_meta']['name'] if 'Ident' in part)
functions=[f for f in data['translated']['fun_decls'] if f]
structured=[f for f in functions if isinstance(f['body'],dict) and 'Structured' in f['body']]
assert sorted(map(function_name,structured))==sorted(profile['structured_bodies'])
assert sum(f['body']=='Opaque' for f in functions)==profile['opaque_function_declarations']
memory=next(t for t in data['translated']['trait_decls'] if t and 'MemoryTr' in str(t['item_meta']['name']))
assert len(memory['types'])==1 and len(memory['types'][0]['params']['regions'])==1
candidate='.local/proof-tools/aeneas-ref-copy-candidate/aeneas'
base='.local/proof-tools/aeneas-source/src/_build/default/main.exe'
with tempfile.TemporaryDirectory(prefix='kasane-revm-borrow-') as tmp:
    input_file=pathlib.Path(tmp)/'actual-add-sub.llbc'
    input_file.write_text(json.dumps(data))
    flags=['-abort-on-error','-warnings-as-errors']
    def run(exe,args):
        return subprocess.run([exe,*args,*flags,str(input_file)],capture_output=True,text=True,timeout=120)
    result=run(base,['-borrow-check'])
    assert result.returncode!=0 and "Can't copy a mutable borrow" in result.stdout+result.stderr,result.stdout+result.stderr
    print('fixed Aeneas: actual mutable-reference copy still rejected')
    result=run(candidate,['-borrow-check'])
    assert result.returncode==0 and 'Crate successfully borrow-checked' in result.stdout+result.stderr,result.stdout+result.stderr
    print('isolated candidate: four structured bodies symbolically borrow-checked; 65 opaque declarations retained')
    result=run(candidate,['-backend','lean','-dest',str(pathlib.Path(tmp)/'lean')])
    log=result.stdout+result.stderr
    assert result.returncode!=0 and 'Found an associated type in a trait declaration' in log and 'MemoryTr' in log and 'SymbolicToPure.ml, line 224' in log,log
    assert not list(pathlib.Path(tmp).rglob('*.lean')),'unsupported translation must not produce accepted Lean output'
    print('candidate Lean: preserved MemoryTr GAT rejected; no instruction proof produced')
check_hashes()
print('[verify-revm-add-sub-borrow-candidate] diagnostic passed; fixed tools unchanged; whole Rust/EVM correspondence unproved')
PYTEST
