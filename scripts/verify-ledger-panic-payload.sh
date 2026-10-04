#!/usr/bin/env bash
# Source panic call arguments are retained; foreign formatting/runtime observations remain modeled.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
repo_root="$PWD"
shasum -a 256 -c proofs/extraction/ledger-time-sources.sha256
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export RUSTFLAGS='-Coverflow-checks=yes'
export CARGO_TARGET_DIR="${repo_root}/.local/proof-tools/ledger-timestamp-target"
export DYLD_LIBRARY_PATH="${repo_root}/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
python3 - <<'PY'
import copy,hashlib,json,pathlib,subprocess,tempfile
profile=json.load(open('proofs/extraction/ledger-panic-payload-profile.json'))
candidate=json.load(open('proofs/extraction/tool-patches/dyn-debug-profile.json'))
fixed=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
def hashes():
 for mapping in [profile['sha256'],profile['rust_primary_sources'],candidate['sha256'],candidate['base_source_sha256'],fixed['experimental_binary_sha256']]:
  for f,h in mapping.items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
hashes()
for tool in [['cargo'],['cargo','miri']]:
 subprocess.run(tool+['test','--manifest-path','proofs/extraction/ledger-time-rust/Cargo.toml','--locked','--test','dyn_debug_source','--test','unwrap_source'],check=True)
with tempfile.TemporaryDirectory(prefix='kasane-panic-payload-') as tmp:
 tmp=pathlib.Path(tmp);dest=tmp/'LedgerPanicPayload.llbc'
 options=['--lift-associated-types','*','--treat-box-as-builtin','--ops-to-function-calls','--index-to-function-calls','--reconstruct-fallible-operations','--reconstruct-asserts','--reconstruct-matches','--hide-marker-traits','--hide-allocator','--remove-unused-self-clauses','--remove-adt-clauses','--unbind-item-vars','--deallocate-all-locals','--no-gen-tuple-structs']
 cmd=['target/debug/charon','cargo']+options+['--mir','optimized','--error-on-warnings','--start-from','kasane_ledger_timestamp_probe::add_time','--start-from','kasane_ledger_timestamp_probe::sub_time']
 for name in ['core::time::Duration','core::num::niche_types::Nanoseconds','core::num::niche_types::_::new_unchecked','core::num::niche_types::_::as_inner','core::time::_::from_nanos','core::time::_::as_nanos','core::time::NANOS_PER_SEC','core::convert::num::_::try_from','core::num::error::TryFromIntError','core::result::_::unwrap','core::result::unwrap_failed']:
  cmd+=['--include',name]
 cmd+=['--opaque','core::panicking::panic_fmt','--dest-file',str(dest),'--','--manifest-path','proofs/extraction/ledger-time-rust/Cargo.toml','--lib','--locked']
 subprocess.run(cmd,check=True)
 data=json.loads(dest.read_text());assert data['has_errors'] is False
 def walk(x):
  yield x
  if isinstance(x,dict):
   for v in x.values():yield from walk(v)
  elif isinstance(x,list):
   for v in x:yield from walk(v)
 assert all(not isinstance(x,dict) or ('Error' not in x and not x.get('has_errors',False)) for x in walk(data['translated']))
 decls=[d for d in data['translated']['fun_decls'] if d]
 bodies=[d for d in decls if isinstance(d['body'],dict) and 'Structured' in d['body']]
 assert len(bodies)==14
 def name(d):return [p['Ident'][0] for p in d['item_meta']['name'] if 'Ident' in p]
 helper=next(d for d in bodies if name(d)==['core','result','unwrap_failed'])
 panic=next(d for d in decls if name(d)==['core','panicking','panic_fmt'])
 assert panic['body']=='Opaque'
 nodes=list(walk(helper['body']))
 assert all(not isinstance(n,dict) or 'Panic' not in n for n in nodes)
 calls=[n['Call']['call'] for n in nodes if isinstance(n,dict) and 'Call' in n]
 assert len(calls)==4,calls
 # Cross-reference the actual panic target, retaining the single formatting argument.
 terminal=calls[-1];assert len(terminal['args'])==1,terminal
 assert terminal['func']['Regular']['kind']['Fun']==panic['def_id'],terminal
 check=subprocess.run(['.local/proof-tools/aeneas-dyn-debug/check',str(dest)],capture_output=True,text=True)
 assert check.returncode==0,check.stdout+check.stderr
 print(check.stdout)
 exe='.local/proof-tools/aeneas-dyn-debug/aeneas'
 def export(path,out,namespace='LedgerPanicPayload'):
  return subprocess.run([exe,'-backend','lean','-abort-on-error','-warnings-as-errors','-split-files','-use-lean-modules','false','-all-computable','-namespace',namespace,'-dest',str(out),str(path)],capture_output=True,text=True)
 out=tmp/'out';out.mkdir();result=export(dest,out)
 assert result.returncode==0,result.stdout+result.stderr
 for f in ['Types.lean','Funs.lean']:
  assert (out/f).read_bytes()==(pathlib.Path('proofs/extraction/operator-lean/LedgerPanicPayload')/f).read_bytes(),f
 generated=(out/'Funs.lean').read_text();assert 'let x ← core.panicking.panic_fmt a2\n  nomatch x' in generated
 # Each serialized Boolean is independently flipped; retain preset=None throughout.
 negatives={k:(not v) for k,v in data['translated']['options'].items() if isinstance(v,bool)}
 negatives.update({'lift_associated_types':[],'mir':None,'rustc_args':['--cfg=unverified'],'include':[]})
 negatives.update({'include':[],'opaque':[],'start_from':[],'preset':'Fast'})
 count=0
 for field,value in negatives.items():
  bad=copy.deepcopy(data);bad['translated']['options'][field]=value
  path=tmp/'negative.llbc';path.write_text(json.dumps(bad));rejected=tmp/'rejected';rejected.mkdir(exist_ok=True)
  r=export(path,rejected)
  log=r.stdout+r.stderr
  assert r.returncode!=0 and 'Invalid option detected' in log,(field,log)
  assert not list(rejected.rglob('*.lean')),field
  count+=1
 print(f'{count} option-profile mutations rejected before Lean output')
 # Re-extract the ordinary preset route; only deliberate panic-profile options differ.
 baseline_dest=tmp/'LedgerUnwrapFailed.llbc'
 suffix=cmd[cmd.index('--mir'):]
 suffix=suffix[:suffix.index('--opaque')]+['--dest-file',str(baseline_dest),'--','--manifest-path','proofs/extraction/ledger-time-rust/Cargo.toml','--lib','--locked']
 subprocess.run(['target/debug/charon','cargo','--preset=aeneas']+suffix,check=True)
 baseline=json.loads(baseline_dest.read_text());assert baseline['has_errors'] is False
 different={k for k,v in baseline['translated']['options'].items() if v!=data['translated']['options'][k]}
 assert different=={'preset','reconstruct_panic_calls','opaque','dest_file'},different
 baseline_helper=next(d for d in baseline['translated']['fun_decls'] if d and name(d)==['core','result','unwrap_failed'])
 panic_nodes=[n['Panic'] for n in walk(baseline_helper['body']) if isinstance(n,dict) and 'Panic' in n]
 assert len(panic_nodes)==1 and 'args' not in panic_nodes[0]
 baseline_out=tmp/'baseline';baseline_out.mkdir()
 r=export(baseline_dest,baseline_out,'LedgerUnwrapFailed')
 assert r.returncode==0,r.stdout+r.stderr
 for f in ['Types.lean','Funs.lean']:
  assert (baseline_out/f).read_bytes()==(pathlib.Path('proofs/extraction/operator-lean/LedgerUnwrapFailed')/f).read_bytes(),f
 print('ordinary Aeneas preset retained; exact option differences and baseline argument loss reproduced')
 # Existing preset candidate cannot consume the retained-argument profile.
 r=subprocess.run(['.local/proof-tools/aeneas-never-call/aeneas','-backend','lean','-abort-on-error',str(dest)],capture_output=True,text=True)
 assert r.returncode!=0 and 'Invalid option detected' in r.stdout+r.stderr
hashes()
PY
cd proofs/extraction/operator-lean
lake build LedgerPanicPayloadAudit LedgerUnwrapFailedAudit
for file in LedgerPanicPayload/Types.lean LedgerPanicPayload/Funs.lean LedgerPanicPayload/FunsExternal.lean LedgerPanicPayloadCorrespondence.lean LedgerPanicPayloadRefinement.lean PanicPayloadCorrespondence.lean LedgerPanicPayloadAudit.lean; do
 lake env lean -DwarningAsError=true "$file"
done
lake env leanchecker LedgerPanicPayloadCorrespondence
lake env leanchecker LedgerPanicPayloadRefinement
lake env leanchecker PanicPayloadCorrespondence
python3 - <<'PY'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('LedgerPanicPayloadAudit.lean').read_text().split('open Lean in',1)
mutations={'axiom':'axiom bad : False\ntheorem negative : False := bad','sorry':'theorem negative : False := by sorry','native':'theorem negative : (1 : Nat) = 1 := by native_decide'}
with tempfile.TemporaryDirectory(prefix='payload-negative-',dir='.lake') as tmp:
 for name,mutation in mutations.items():
  f=pathlib.Path(tmp)/f'{name}.lean'
  f.write_text(head+'import Std.Tactic\nnamespace PanicPayloadCorrespondence\n'+mutation+'\nend PanicPayloadCorrespondence\nopen Lean in'+body)
  result=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert result.returncode!=0 and 'depends on forbidden axiom' in result.stdout+result.stderr,result.stdout+result.stderr
  print(name+' rejected by panic payload audit')
PY
echo '[verify-ledger-panic-payload] unchanged 14-body source export; retained panic call argument; 40 named conditional correspondence/refinement theorems; 153 declarations audited; Rust/Miri and kernel/negative checks passed. Formatting, unwinding and external runtime correspondence remain unproved.'
