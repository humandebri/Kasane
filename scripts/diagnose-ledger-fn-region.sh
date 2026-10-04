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
import copy,hashlib,json,pathlib,subprocess,tempfile
fixed=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
profile=json.load(open('proofs/extraction/tool-patches/fn-region-profile.json'))
primary=json.load(open('proofs/extraction/ledger-panic-payload-profile.json'))['rust_primary_sources']
def hashes():
 for mapping in [fixed['experimental_binary_sha256'],profile['sha256'],profile['base_source_sha256'],primary]:
  for f,h in mapping.items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
hashes()
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
 result=subprocess.run(['.local/proof-tools/aeneas-fn-region/check',str(dest)],capture_output=True,text=True)
 log=result.stdout+result.stderr
 assert result.returncode==0 and '29 occurrences' in log and 'every other Lean mapping unchanged' in log and 'closed signatures retained exactly: 13; open signatures rejected: 16' in log and '12 malformed/open signatures and three non-Lean backends rejected' in log and '4 FnDef, 29 FnPtr' in log and '36 depth/index scope cases' in log and 'two source trait reification signatures match exactly; missing trait/method/poly trait rejected; unsafe/ABI/variadic distinct' in log,log
 print(result.stdout)
 out=tmp/'out';out.mkdir()
 exe='.local/proof-tools/aeneas-fn-region/aeneas'
 args=['-backend','lean','-abort-on-error','-warnings-as-errors','-split-files','-use-lean-modules','false','-all-computable','-dest',str(out)]
 result=subprocess.run([exe]+args+[str(dest)],capture_output=True,text=True)
 log=result.stdout+result.stderr
 assert result.returncode!=0 and 'Invalid input for unop: cast<for<' in log and 'fmt/rt.rs' in log and 'Invalid input for unop: transmute' in log and 'fmt/mod.rs' in log and '14/17' in log,log
 assert 'Function value rty rejected' not in log and 'Internal error' not in log,log
 assert 'Arrow types are not supported yet' not in log,log
 assert not list(out.rglob('*.lean')),list(out.rglob('*.lean'))
 predecessor=subprocess.run(['.local/proof-tools/aeneas-closed-fnptr/aeneas']+args+[str(dest)],capture_output=True,text=True)
 assert predecessor.returncode!=0 and 'interp/InterpUtils.ml' in predecessor.stdout+predecessor.stderr
 assert not list(out.rglob('*.lean'))
 # A weakened input profile must stop at the options check, before type translation.
 for key in ['skip_borrowck','no_typecheck','no_normalize','erase_body_lifetimes','reconstruct_panic_calls']:
  bad=copy.deepcopy(data);bad['translated']['options'][key]=True
  f=tmp/'bad.llbc';f.write_text(json.dumps(bad))
  r=subprocess.run([exe]+args+[str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'Invalid option detected' in r.stdout+r.stderr,(key,r.stdout+r.stderr)
  assert not list(out.rglob('*.lean'))
 print('17 clean source bodies; exact closed Rust signature metadata retained in 13 occurrences, 16 open occurrences and 12 bad signatures/three backends rejected; safe/unsafe distinct and closed substitution unchanged. Type declaration stage passed; scoped function rty and erasure preserved; trait-item reification and reference-pointer casts strictly unsupported; five weakened options rejected. No Lean export or correspondence proof claimed.')
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
