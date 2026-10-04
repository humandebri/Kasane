#!/usr/bin/env bash
# Reproduce the next source-level obligation without accepting a modeled formatter replacement.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/ledger-timestamp-target"
export RUSTFLAGS='-Coverflow-checks=yes'
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
shasum -a 256 -c proofs/extraction/ledger-time-sources.sha256
python3 - <<'PY'
import copy,hashlib,json,os,pathlib,subprocess,tempfile
fixed=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
profile=json.load(open('proofs/extraction/tool-patches/fn-item-value-profile.json'))
primary=json.load(open('proofs/extraction/ledger-panic-payload-profile.json'))['rust_primary_sources']
def hashes():
 for mapping in [fixed['experimental_binary_sha256'],profile['sha256'],profile['base_source_sha256'],primary]:
  for f,h in mapping.items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
hashes()
for tool in [['cargo'],['cargo','miri']]:
 subprocess.run(tool+['test','--manifest-path','proofs/extraction/ledger-time-rust/Cargo.toml','--locked','--test','fn_value_source','--test','fn_transmute_source'],check=True)
with tempfile.TemporaryDirectory(prefix='fn-transmute-rust-negative-') as neg:
 for name,code in [('capture','E0308'),('return_borrow','E0597'),('outer_borrow','E0506')]:
  source='proofs/extraction/tool-patches/tests/fn_value_'+name+'_fail.rs'
  r=subprocess.run(['rustc','--edition=2021','--error-format=json',source,'--out-dir',neg],capture_output=True,text=True)
  errors=[json.loads(line) for line in r.stderr.splitlines() if line.startswith('{')]
  assert r.returncode!=0 and any(e.get('code') and e['code'].get('code')==code for e in errors),(name,r.stderr)
  assert not list(pathlib.Path(neg).iterdir())
  print(name,code,'rejected by fixed Rust')
with tempfile.TemporaryDirectory(prefix='fn-transmute-receiver-miri-') as neg:
 source=pathlib.Path('proofs/extraction/tool-patches/tests/fn_transmute_receiver_fail.rs').resolve()
 manifest=pathlib.Path(neg)/'Cargo.toml'
 manifest.write_text('[package]\nname="fn-transmute-receiver-negative"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[lib]\npath='+json.dumps(str(source))+'\n')
 r=subprocess.run(['cargo','miri','test','--manifest-path',str(manifest)],capture_output=True,text=True,env={**os.environ,'CARGO_TARGET_DIR':str(pathlib.Path(neg)/'target')})
 assert r.returncode!=0 and 'Undefined Behavior' in r.stdout+r.stderr and 'invalid value of type &u64' in r.stdout+r.stderr and 'fn_transmute_receiver_fail.rs' in r.stdout+r.stderr,r.stdout+r.stderr
 print('Miri-only wrong receiver control rejected: u8 value paired with u64 callback')
cmd=['target/debug/charon','cargo','--lift-associated-types','*']
for flag in ['treat-box-as-builtin','ops-to-function-calls','index-to-function-calls','reconstruct-fallible-operations','reconstruct-asserts','reconstruct-matches','hide-marker-traits','hide-allocator','remove-unused-self-clauses','remove-adt-clauses','unbind-item-vars','deallocate-all-locals','no-gen-tuple-structs']:
 cmd+=['--'+flag]
cmd+=['--mir','optimized','--error-on-warnings','--start-from','kasane_ledger_timestamp_probe::add_time','--start-from','kasane_ledger_timestamp_probe::sub_time']
for item in ['core::time::Duration','core::num::niche_types::Nanoseconds','core::num::niche_types::_::new_unchecked','core::num::niche_types::_::as_inner','core::time::_::from_nanos','core::time::_::as_nanos','core::time::NANOS_PER_SEC','core::convert::num::_::try_from','core::num::error::TryFromIntError','core::result::_::unwrap','core::result::unwrap_failed','core::fmt::rt::Argument','core::fmt::rt::ArgumentType','core::fmt::Arguments','core::fmt::rt::_::new_debug','core::fmt::rt::_::new_display','core::fmt::_::new']:
 cmd+=['--include',item]
with tempfile.TemporaryDirectory(prefix='kasane-formatting-source-') as tmp:
 tmp=pathlib.Path(tmp);dest=tmp/'LedgerFormattingSource.llbc'
 subprocess.run(cmd+['--opaque','core::panicking::panic_fmt','--dest-file',str(dest),'--','--manifest-path','proofs/extraction/ledger-time-rust/Cargo.toml','--lib','--locked'],check=True)
 data=json.loads(dest.read_text());assert data['has_errors'] is False
 def walk(x):
  yield x
  if isinstance(x,dict):
   for v in x.values():yield from walk(v)
  elif isinstance(x,list):
   for v in x:yield from walk(v)
 assert all(not isinstance(x,dict) or ('Error' not in x and not x.get('has_errors',False)) for x in walk(data['translated']))
 def name(d):return [p['Ident'][0] for p in d['item_meta']['name'] if 'Ident'in p]
 bodies=[d for d in data['translated']['fun_decls'] if d and isinstance(d['body'],dict) and 'Structured'in d['body']]
 assert len(bodies)==17
 for n in [['core','fmt','rt','new_debug'],['core','fmt','rt','new_display'],['core','fmt','new']]:
  assert len([d for d in bodies if name(d)==n])==1,n
 funcs={d['def_id']:d for d in data['translated']['fun_decls'] if d}
 for d in bodies:
  n=name(d)
  if n not in [['core','fmt','rt','new_debug'],['core','fmt','rt','new_display'],['core','fmt','new']]:continue
  nodes=list(walk(d['body']))
  casts=[next(iter(x['Cast'])) for x in nodes if isinstance(x,dict) and 'Cast'in x]
  calls=[x['Call']['call'] for x in nodes if isinstance(x,dict) and 'Call'in x]
  if n[-1]=='new':
   assert casts==['Transmute','Transmute'] and not calls,(n,casts,calls)
  else:
   assert casts==['FnPtr','Transmute'],(n,casts)
   assert [name(funcs[c['func']['Regular']['kind']['Fun']]) for c in calls]==[
    ['core','ptr','non_null','from_ref'],['core','ptr','non_null','cast']],calls
 print('both argument constructors retain NonNull receiver calls, trait-method reification and function-pointer transmute; Arguments.new retains both pointer transmutes')
 types={tuple(name(d)):d for d in data['translated']['type_decls'] if d}
 assert len(types[('core','fmt','rt','Argument')]['kind']['Struct'])==1
 variants=types[('core','fmt','rt','ArgumentType')]['kind']['Enum']
 assert [v['name'] for v in variants]==['Placeholder','Count']
 assert [f['name'] for f in variants[0]['fields']]==['value','formatter','_lifetime']
 assert [f['name'] for f in types[('core','fmt','Arguments')]['kind']['Struct']]==['template','args']
 result=subprocess.run(['.local/proof-tools/aeneas-fn-item-value/check',str(dest)],capture_output=True,text=True)
 log=result.stdout+result.stderr
 assert result.returncode==0 and '29 occurrences' in log and 'every other Lean mapping unchanged' in log and 'closed signatures retained exactly: 13; open signatures rejected: 16' in log and '12 malformed/open signatures and three non-Lean backends rejected' in log and '4 FnDef, 29 FnPtr' in log and '36 depth/index scope cases' in log and 'two actual reification guards accepted; 14 target mutations rejected' in log and '33 actual function region inventories contain no free lifetimes' in log and 'function ety scope checked' in log and '29 actual function pointers have no stored signature borrows; 58 shared/mutable outer and adjacent contexts plus nested contexts retained' in log and 'two actual function-pointer transmutes admitted only as retained representation casts; 22 source/target controls rejected' in log and '33 actual function region predicates preserve local/free distinction' in log and 'two source trait reification signatures match exactly; missing trait/method/poly trait rejected; unsafe/ABI/variadic distinct' in log and '20 free-variable rebinding roundtrips' in log and '60 unsupported item shapes rejected' in log and 'retained Pure coercion visits both endpoint type variables and preserves direction' in log and '16 actual open pointer templates reconstruct exact source' in log and '192 unsupported template shapes rejected' in log and '64 injective free-variable renamings' in log and 'two-variable repeated/distinct slot control retained' in log and 'two actual Pure transmute operators retain endpoints and kind' in log and '22 representation negatives and three other backends rejected before type translation' in log and 'two actual trait item constants retain identity/type binders/erased value regions' in log and '12 value and 18 type controls rejected' in log,log
 print(result.stdout)
 out=tmp/'out';out.mkdir()
 exe='.local/proof-tools/aeneas-fn-item-value/aeneas'
 args=['-backend','lean','-abort-on-error','-warnings-as-errors','-split-files','-use-lean-modules','false','-all-computable','-dest',str(out)]
 result=subprocess.run([exe]+args+[str(dest)],capture_output=True,text=True)
 log=result.stdout+result.stderr
 assert result.returncode!=0 and '16/17' in log and 'Invalid input for unop: transmute<&' in log and 'fmt/mod.rs' in log and 'interp/InterpExpressions.ml, line 1047' in log,log
 assert 'Trait function-item value identity/instantiation mismatch' not in log and 'Trait function-item value expected type mismatch' not in log and 'Malformed trait function-item value captures' not in log and 'symbolic/SymbolicToPureExpressions.ml, line 1681' not in log and 'Unimplemented' not in log,log
 assert 'Unsupported: transmute' not in log and 'Unsupported Pure function-pointer transmute representation' not in log and 'Pure function-pointer transmute operand mismatch' not in log,log
 assert 'Unsupported open or non-Rust function pointer signature' not in log and 'Unsupported Rust function-pointer template' not in log and 'Pure function-item coercion operand mismatch' not in log,log
 assert 'TODO: function casts' not in log and 'Unsupported Pure function-item coercion signature' not in log,log
 assert 'symbolic/SymbolicToPureTypes.ml, line 487' not in log,log
 assert 'Invalid input for unop: transmute<fn<' not in log and 'Unsupported function-pointer transmute representation' not in log,log
 assert 'Unreachable' not in log and 'interp/InterpExpansion.ml' not in log,log
 assert 'Invalid input for unop: cast<for<' not in log and 'Unsupported function item reification signature' not in log and 'region_in_set' not in log and 'new value doesn' not in log,log
 assert 'Function value rty rejected' not in log and 'Internal error' not in log,log
 assert 'Arrow types are not supported yet' not in log,log
 assert not list(out.rglob('*.lean')),list(out.rglob('*.lean'))
 predecessor=subprocess.run(['.local/proof-tools/aeneas-fn-pure-transmute/aeneas']+args+[str(dest)],capture_output=True,text=True)
 assert predecessor.returncode!=0 and 'Unimplemented' in predecessor.stdout+predecessor.stderr and 'symbolic/SymbolicToPureExpressions.ml, line 1681' in predecessor.stdout+predecessor.stderr
 assert not list(out.rglob('*.lean'))
 # Mutate only the interned destination signature of one real cast.
 casts=[x['Cast']['FnPtr'] for x in walk(data['translated']['fun_decls']) if isinstance(x,dict) and isinstance(x.get('Cast'),dict) and 'FnPtr' in x['Cast']]
 assert len(casts)==2
 dest_ty=casts[0][1]
 dest_id=dest_ty['Deduplicated'] if 'Deduplicated' in dest_ty else dest_ty['Value'][0]
 for key,value in [('abi','C'),('is_unsafe',True),('is_variadic',True)]:
  bad=copy.deepcopy(data)
  definitions=[x['Value'][1] for x in walk(bad) if isinstance(x,dict) and 'Value'in x and isinstance(x['Value'],list) and len(x['Value'])==2 and x['Value'][0]==dest_id and isinstance(x['Value'][1],dict) and 'FnPtr' in x['Value'][1]]
  assert len(definitions)==1 and 'FnPtr' in definitions[0],definitions
  signature=definitions[0]['FnPtr']['skip_binder']
  assert signature[key]!=value
  signature[key]=value
  f=tmp/'bad-signature.llbc';f.write_text(json.dumps(bad))
  r=subprocess.run([exe]+args+[str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'Unsupported function item reification signature' in r.stdout+r.stderr,(key,r.stdout+r.stderr)
  assert not list(out.rglob('*.lean'))
 print('three actual cast destination mutations rejected by symbolic reification guard before callable export')
 # Give only the cast target an uninterned mutated type; the stored field type remains intact.
 def type_id(ty):return ty['Deduplicated'] if 'Deduplicated'in ty else ty['Value'][0]
 def definition(ty):
  if 'Value'in ty:return ty['Value'][1]
  if 'Untagged'in ty:return ty['Untagged']
  defs=[x['Value'][1] for x in walk(data) if isinstance(x,dict) and 'Value'in x and isinstance(x['Value'],list) and len(x['Value'])==2 and x['Value'][0]==type_id(ty) and isinstance(x['Value'][1],dict) and 'FnPtr'in x['Value'][1]]
  return defs[0] if len(defs)==1 else None
 trans=[x['Cast']['Transmute'] for x in walk(data['translated']['fun_decls']) if isinstance(x,dict) and isinstance(x.get('Cast'),dict) and 'Transmute'in x['Cast'] and definition(x['Cast']['Transmute'][0]) is not None and 'FnPtr'in definition(x['Cast']['Transmute'][0])]
 assert len(trans)==2
 target=definition(trans[0][1]);assert target and 'FnPtr'in target
 def cached_types(x):
  if isinstance(x,dict):
   if 'Value'in x and isinstance(x['Value'],list) and len(x['Value'])==2:return {'Deduplicated':x['Value'][0]}
   return {k:cached_types(v) for k,v in x.items()}
  if isinstance(x,list):return [cached_types(v) for v in x]
  return x
 for key,value in [('abi','C'),('is_unsafe',False),('is_variadic',True)]:
  bad=copy.deepcopy(data)
  bad_trans=[x['Cast']['Transmute'] for x in walk(bad['translated']['fun_decls']) if isinstance(x,dict) and isinstance(x.get('Cast'),dict) and 'Transmute'in x['Cast'] and definition(x['Cast']['Transmute'][0]) is not None and 'FnPtr'in definition(x['Cast']['Transmute'][0])]
  bad_type=cached_types(copy.deepcopy(target));bad_type['FnPtr']['skip_binder'][key]=value
  bad_trans[0][1]={'Untagged':bad_type}
  f=tmp/'bad-transmute.llbc';f.write_text(json.dumps(bad))
  r=subprocess.run([exe]+args+[str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'Unsupported function-pointer transmute representation' in r.stdout+r.stderr,(key,r.stdout+r.stderr)
  assert not list(out.rglob('*.lean'))
 print('three actual FnPtr transmute target mutations rejected by representation guard; no call/receiver proof inferred')
 # A weakened input profile must stop at the options check, before type translation.
 for key in ['skip_borrowck','no_typecheck','no_normalize','erase_body_lifetimes','reconstruct_panic_calls']:
  bad=copy.deepcopy(data);bad['translated']['options'][key]=True
  f=tmp/'bad.llbc';f.write_text(json.dumps(bad))
  r=subprocess.run([exe]+args+[str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'Invalid option detected' in r.stdout+r.stderr,(key,r.stdout+r.stderr)
  assert not list(out.rglob('*.lean'))
 print('17 clean source bodies; two exact item reifications admitted as retained symbolic casts; 14 native target mutations and three actual symbolic target mutations rejected. Function binder/free-region inventory and value/ety scope preserved; 29 actual pointer signatures do not count as stored borrows while 58 shared/mutable outer and adjacent contexts plus nested contexts retain their flags. Two function-pointer transmutes retained symbolically, 22 native representation controls and three actual target mutations rejected. Four trait function-item templates preserve source metadata and substitution-visible Self/evidence captures; 20 rebinding roundtrips and 60 unsupported-shape rejections checked. Pure coercion branch checks exact source signature, tracks both endpoint types and rejects callable extraction; two synthetic endpoint traversal controls passed. Sixteen source open pointer templates retain exact signatures with explicit captures; substitution/reconstruction, 64 alpha-renamings and 192 bad shapes checked. Actual Pure pointer transmutes retain both endpoint types and source captures with representation-only signature and operand checks; Native actual2 operators/substitution/22 source-target negatives/backend rejection verified. Two real trait-item constants preserve type binder, identity, erased value regions and substitution-visible Self/evidence; actual prepass results used. Native12 value/18 qualifier-type controls,wrong expected types/non-Lean backends rejected. Source pipeline now16/17: both argument constructors pass previous value stop. Remaining stop is Arguments.new reference-to-NonNull transmute at InterpExpressions1047. Predecessor trait function-item value failure and five weakened options reproduced. Zero Lean output; no new Rust correspondence or callable runtime proof claimed.')
hashes()
PY

cd proofs/extraction/operator-lean
lake build FnRegionErasureAudit
lake env lean -DwarningAsError=true FnRegionErasure.lean
lake env lean -DwarningAsError=true FnRegionErasureAudit.lean
lake env leanchecker FnRegionErasure
python3 - <<'PYNEG'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('FnRegionErasureAudit.lean').read_text().split('open Lean in',1)
mutations={'axiom':'axiom bad : False\ntheorem negative : False := bad',
 'sorry':'theorem negative : False := by sorry',
 'native':'theorem negative : (1 : Nat) = 1 := by native_decide'}
with tempfile.TemporaryDirectory(prefix='region-negative-',dir='.lake') as tmp:
 for name,mutation in mutations.items():
  f=pathlib.Path(tmp)/f'{name}.lean'
  f.write_text(head+'import Std.Tactic\nnamespace FnRegionErasure\n'+mutation+'\nend FnRegionErasure\nopen Lean in'+body)
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'depends on forbidden axiom' in r.stdout+r.stderr,r.stdout+r.stderr
  print(name+' rejected by region model audit')
PYNEG
