#!/usr/bin/env bash
# Conditional footprint model and unchanged-failure diagnostic; no candidate adoption.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/gat-override-target"
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
python3 - <<'PYTEST'
import hashlib,json,pathlib,subprocess,tempfile
profiles=[json.load(open('proofs/extraction/tool-patches/'+name)) for name in
          ['gat-borrow-obligations-profile.json','rpit-signature-profile.json','gat-closed-item-equality-profile.json','revm-add-sub-borrow-profile.json']]
fixed=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
def check_hashes():
    for p in profiles:
        for f,h in p['sha256'].items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
    for f,h in fixed['experimental_binary_sha256'].items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
check_hashes()
with tempfile.TemporaryDirectory(prefix='kasane-gat-footprint-') as tmp:
    tmp=pathlib.Path(tmp)
    dest=tmp/'override.llbc'
    cmd=['.local/proof-tools/charon-rpit-signature-candidate/charon','cargo','--preset=aeneas','--mir','promoted',
         '--monomorphize-mut=all','--error-on-warnings','--dest-file',str(dest),'--',
         '--manifest-path','proofs/extraction/gat-override-rust/Cargo.toml','--lib','--locked']
    result=subprocess.run(cmd,capture_output=True,text=True,timeout=120)
    assert result.returncode==0,result.stdout+result.stderr
    data=json.loads(dest.read_text());assert data['has_errors'] is False
    def clean(x):
        if isinstance(x,dict):
            assert not x.get('has_errors',False) and 'Error' not in x,x
            for c in x.values():clean(c)
        elif isinstance(x,list):
            for c in x:clean(c)
    clean(data['translated'])
    check=subprocess.run(['.local/proof-tools/gat-borrow-analysis/check',str(dest)],capture_output=True,text=True,timeout=120)
    check_log=check.stdout+check.stderr
    assert check.returncode==0 and 'actual default signature contains_borrow=true; corresponding declared GAT contains_borrow=false' in check_log,check_log
    print('original actual type analysis: concrete borrowed default true, corresponding abstract GAT false')
    result=subprocess.run(['.local/proof-tools/aeneas-gat-diagnostic/aeneas','-borrow-check',
        '-abort-on-error','-warnings-as-errors','-log','InterpBorrows',str(dest)],capture_output=True,text=True,timeout=120)
    log=result.stdout+result.stderr
    assert result.returncode!=0 and 'GAT comparison diagnostic:' in log and 'deref.rs' in log
    import re
    pairs=re.findall(r'- first: TraitClause0::slice_len_ty<([^>]+)>\n- second: TraitClause0::slice_len_ty<([^>]+)>',log)
    assert pairs and any(a!=b for a,b in pairs),log
    assert '- erased types equal: true' in log
    assert '- first contains_borrow: false' in log and '- second contains_borrow: false' in log
    assert 'Internal error, please file an issue' in log
    print('actual GAT views: distinct fresh lifetimes, equal erased types, both borrow flags false; original strict failure preserved')
check_hashes()
PYTEST
cd proofs/extraction/operator-lean
lake build GatBorrowFootprint GatBorrowAudit
lake env lean -DwarningAsError=true GatBorrowFootprint.lean
lake env lean -DwarningAsError=true GatBorrowAudit.lean
lake env leanchecker GatBorrowFootprint
python3 - <<'PYTEST'
import pathlib,subprocess,tempfile
src=pathlib.Path('GatBorrowAudit.lean').read_text()
head,body=src.split('open Lean in',1)
mutations={
 'axiom':'namespace GatBorrowFootprint\naxiom bad : False\ntheorem negative : False := bad\nend GatBorrowFootprint\n',
 'sorry':'namespace GatBorrowFootprint\ntheorem negative : False := by sorry\nend GatBorrowFootprint\n',
 'native':'namespace GatBorrowFootprint\ntheorem negative : (1 : Nat) = 1 := by native_decide\nend GatBorrowFootprint\n'}
with tempfile.TemporaryDirectory(prefix='gat-footprint-negative-',dir='.lake') as tmp:
    for name,mutation in mutations.items():
        file=pathlib.Path(tmp)/f'{name}.lean'
        file.write_text(head+'import Std.Tactic\n'+mutation+'open Lean in'+body)
        result=subprocess.run(['lake','env','lean',str(file)],capture_output=True,text=True)
        output=result.stdout+result.stderr
        assert result.returncode!=0 and 'depends on forbidden axiom' in output,output
        print(name+' dependency rejected by GAT footprint audit')
PYTEST
echo '[verify-gat-borrow-obligations] actual failure diagnosed, eleven conditional model theorems audited/kernel-checked and three negatives rejected; Rust/LLBC/GAT semantics remain unproved'
