#!/usr/bin/env bash
# Full fixed instruction-table extraction diagnostic, not an EVM proof gate.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/revm-instruction-target"
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
python3 - "${1:-closed-gat}" <<'PYTEST'
import hashlib,json,pathlib,re,subprocess,tempfile,sys
profile=json.load(open('proofs/extraction/tool-patches/revm-all-instruction-profile.json'))
profiles=[profile,json.load(open('proofs/extraction/tool-patches/gat-closed-item-equality-profile.json')),
          json.load(open('proofs/extraction/tool-patches/revm-add-sub-borrow-profile.json'))]
fixed=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
mode=sys.argv[1]
assert mode in ['closed-gat','rpit-signature'],mode
charon='.local/proof-tools/charon-gat-candidate/charon'
if mode=='rpit-signature':
    profiles.append(json.load(open('proofs/extraction/tool-patches/rpit-signature-profile.json')))
    charon='.local/proof-tools/charon-rpit-signature-candidate/charon'
def check_hashes():
    for p in profiles:
        for f,h in p['sha256'].items():
            assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
    for f,h in fixed['experimental_binary_sha256'].items():
        assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
check_hashes()
root=pathlib.Path('proofs/extraction/revm-instruction-rust')
src=pathlib.Path('vendor/revm/crates/interpreter/src/instructions.rs').read_text()
body=src.split('const fn instruction_table_impl',1)[1].split('\n#[cfg(test)]',1)[0]
entries=re.findall(r'table\[([A-Z0-9]+) as usize\] = Instruction::new\((\w+::\w+(?:::\s*<[^>]+>)?),\s*([^;]+)\);',body)
assert len(entries)==len(re.findall(r'table\[',body))==profile['table']['explicit_table_entries']
assert 'let mut table = [Instruction::unknown(); 256];' in body
opcodes=pathlib.Path('vendor/revm/crates/bytecode/src/opcode.rs').read_text()
values={name:int(num,16) for num,name in re.findall(r'^\s*(0x[0-9A-Fa-f]+)\s*=>\s*([A-Z0-9]+)\s*=>',opcodes,re.M)}
assert len(values)==len(set(values.values()))==profile['table']['declared_opcode_bytes']
assert len({op for op,_,_ in entries})==len(entries)
assert set(values)=={op for op,_,_ in entries}
assert all(0<=value<256 for value in values.values())
assert 256-len(values)==profile['table']['default_unknown_bytes']
expected_entries=[dict(opcode_name=o,actual_function=f,base_static_gas_expression=g,opcode_byte=values[o]) for o,f,g in entries]
assert json.loads((root/'table-entries.json').read_text())==expected_entries
lines=['// Diagnostic corpus generated from the fixed actual revm instruction table.',
       '// DummyHost is a probe type; host implementation/refinement is not proved.',
       'use revm_interpreter::{host::DummyHost, interpreter::EthInterpreter, InstructionContext, Interpreter};','']
for op,callee,cost in entries+[('UNKNOWN','control::unknown','0')]:
    lines.extend([f'pub fn opcode_{op.lower()}(interpreter: &mut Interpreter<EthInterpreter>, host: &mut DummyHost) {{',
        f'    revm_interpreter::instructions::{callee}(InstructionContext {{ interpreter, host }});','}',''])
def tokens(text):
    text=re.sub(r',\s*([})])',r'\1',text)
    return re.findall(r'//[^\n]*|\w+|[^\s]',text)
assert tokens((root/'lib.rs').read_text())==tokens('\n'.join(lines)),'corpus must match every exact callee and const argument'
functions=sorted({e['actual_function'].split('::<')[0] for e in expected_entries}|{'control::unknown'})
assert len(functions)==profile['table']['distinct_instruction_bodies_including_unknown']
print('coverage: 150 exact table entries/declared bytes and 106 default unknown bytes; 151 concrete wrappers')
manifest=str(root/'Cargo.toml')
with tempfile.TemporaryDirectory(prefix='kasane-all-instructions-') as tmp:
    tmp=pathlib.Path(tmp)
    def run(cmd,logfile):
        with logfile.open('w') as out:
            result=subprocess.run(cmd,stdout=out,stderr=subprocess.STDOUT,timeout=180)
        return result.returncode,logfile.read_text()
    rc,log=run(['cargo','check','--manifest-path',manifest,'--lib','--locked'],tmp/'native.log')
    assert rc==0,log
    input_file=tmp/'all-instructions.llbc'
    cmd=[charon,'cargo','--preset=aeneas','--mir','promoted',
         '--monomorphize-mut=all','--error-on-warnings']
    for f in functions:cmd+=['--include','revm_interpreter::instructions::'+f]
    cmd+=['--dest-file',str(input_file),'--','--manifest-path',manifest,'--lib','--locked']
    rc,log=run(cmd,tmp/'charon.log')
    assert rc==0,log
    data=json.loads(input_file.read_text())
    assert data['has_errors'] is False
    def clean(x):
        if isinstance(x,dict):
            assert not x.get('has_errors',False) and 'Error' not in x,x
            for c in x.values():clean(c)
        elif isinstance(x,list):
            for c in x:clean(c)
    clean(data['translated'])
    def name(f):return '::'.join(p['Ident'][0] for p in f['item_meta']['name'] if 'Ident' in p)
    declarations=[f for f in data['translated']['fun_decls'] if f]
    structured=[f for f in declarations if isinstance(f['body'],dict) and 'Structured' in f['body']]
    expected={'revm_interpreter::instructions::'+f for f in functions}
    expected|={'kasane_revm_instruction_probe::opcode_'+o.lower() for o,_,_ in entries+[('UNKNOWN','','')]}
    assert len(structured)==profile['table']['structured_bodies_including_wrappers']
    assert set(map(name,structured))==expected,'actual instruction or wrapper body missing'
    assert sum(f['body']=='Opaque' for f in declarations)==profile['table']['opaque_function_declarations']
    memory=next(t for t in data['translated']['trait_decls'] if t and 'MemoryTr' in str(t['item_meta']['name']))
    assert len(memory['types'])==1 and len(memory['types'][0]['params']['regions'])==1
    assoc=memory['types'][0]
    assert assoc['params']['types_outlive'] and assoc['params']['trait_type_constraints']
    assert assoc['skip_binder']['implied_clauses'],'GAT constraints must remain'
    default_method=next(f for f in declarations if name(f)=='revm_interpreter::interpreter_types::MemoryTr::slice_len')
    assert default_method['body']=='Opaque' and 'TraitDefault' in default_method['src']
    output=default_method['signature']['output']['Value'][1]['Adt']
    default_ty=assoc['skip_binder']['default']['value']['Value'][1]['Adt']
    assert output['id']==default_ty['id']
    ref_decl=next(t for t in data['translated']['type_decls'] if t and t['def_id']==output['id'])
    assert name(ref_decl)=='core::cell::Ref'
    assert len(output['generics']['regions'])==len(default_ty['generics']['regions'])==1
    method=next(m['skip_binder'] for m in memory['methods'] if m['skip_binder']['name']=='slice_len')
    method_output=method['signature']['output']
    default_value=assoc['skip_binder']['default']['value']['Value']
    if mode=='closed-gat':
        assert method_output.get('Deduplicated')==default_value[0] or method_output.get('Value')==default_value
    else:
        projection=method_output['Value'][1]['TraitType']
        assert projection[1]==0
        assert projection[2]==dict(regions=[{'Var':{'Bound':[0,0]}}],types=[],const_generics=[],trait_refs=[])
    print('opaque default method signature/default associated value are Ref; outlives, Deref and GAT equality constraints retained')
    print('fresh Charon extraction: all 85 actual instruction bodies and 151 wrappers clean; 273 opaque declarations; GAT binder retained')
    rc,log=run(['.local/proof-tools/aeneas-ref-copy-candidate/aeneas','-borrow-check','-abort-on-error',
         '-warnings-as-errors','-log','InterpPaths',str(input_file)],tmp/'borrow.log')
    assert rc!=0,log[-10000:]
    if mode=='closed-gat':
        assert 'new value doesn\'t have the same type as its destination' in log,log[-10000:]
        assert profile['borrow_check']['actual_value_type'] in log and profile['borrow_check']['expected_destination_type'] in log
        assert 'interpreter_types.rs' in log and 'InterpPaths.ml, line 260' in log
        print('borrow diagnostic: Ref<[u8]> vs MemoryTr slice_len_ty retained; strict type mismatch rejection, no proof output')
    else:
        assert 'InterpPaths.ml, line 260' not in log
        assert 'Internal error, please file an issue' in log and 'deref.rs' in log and 'InterpBorrowsCore.ml, line 476' in log,log[-10000:]
        print('declared return candidate: call type mismatch passed; Deref/GAT projection comparison still rejects; no instruction proof')
check_hashes()
print('[diagnose-revm-instructions] full-table fresh extraction diagnostic passed; candidate correctness, full instruction and external-runtime correspondence unproved')
PYTEST
