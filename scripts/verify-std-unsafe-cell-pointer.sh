#!/usr/bin/env bash
# Actual-source scalar pointer IR; physical Rust/Cell/layout correspondence unproved.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/revm-root-target"
python3 - <<'RUN'
import hashlib,json,pathlib,subprocess,sys,tempfile
sys.path.insert(0,'scripts')
from extract_std_unsafe_cell_pointer import lower,render,negative_controls
profile=json.load(open('proofs/extraction/tool-patches/std-unsafe-cell-pointer-profile.json'))
for mapping in [profile['sha256'],profile['base_source_sha256']]:
 for f,h in mapping.items(): assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
with tempfile.TemporaryDirectory(prefix='kasane-std-unsafe-cell-pointer-') as tmp:
 dest=pathlib.Path(tmp)/'cell.llbc'
 args=['.local/proof-tools/charon-gat-candidate/charon','cargo','--preset=aeneas','--mir','promoted','--error-on-warnings','--start-from','core::cell::BorrowRef::new']
 for name in ['core::cell::_::new','core::cell::_::get','core::cell::_::replace','core::cell::BorrowRef','core::cell::Cell','core::cell::UnsafeCell','core::cell::is_reading','core::cell::UNUSED','core::mem::replace','core::ptr::read','core::ptr::write']:args+=['--include',name]
 args+=['--dest-file',str(dest),'--','--manifest-path','crates/evm-core/Cargo.toml','--lib','--locked']
 r=subprocess.run(args,capture_output=True,text=True,timeout=180);assert r.returncode==0,r.stdout+r.stderr
 data=json.loads(dest.read_text());ops=lower(data);negative_controls(data)
 assert ops==profile['operations']
 assert render(ops)==pathlib.Path('proofs/extraction/u256-lean/StdUnsafeCellGenerated.lean').read_text(),'Actual source pointer IR drift'
print('Fresh actual UnsafeCell layout and all nineteen pointer statements matched; seven source/schema modifications rejected.')
RUN
cd proofs/extraction/u256-lean
for source in StdUnsafeCellPointer StdUnsafeCellGenerated StdUnsafeCellCorrespondence StdUnsafeCellAudit; do
 lake env lean -DwarningAsError=true -o ".lake/build/lib/lean/$source.olean" "$source.lean"
done
lake env leanchecker StdUnsafeCellCorrespondence
python3 - <<'NEG'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('StdUnsafeCellAudit.lean').read_text().split('open Lean in',1)
with tempfile.TemporaryDirectory(prefix='unsafe-cell-negative-',dir='.lake') as tmp:
 for label,mutation in {
  'axiom':'axiom bad : False\ntheorem negative : False := bad',
  'sorry':'theorem negative : False := by sorry',
  'native':'theorem negative : (1 : Nat) = 1 := by native_decide'}.items():
  f=pathlib.Path(tmp)/f'{label}.lean';f.write_text(head+'import Std.Tactic\nnamespace StdUnsafeCellPointer\n'+mutation+'\nend StdUnsafeCellPointer\nopen Lean in'+body)
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'depends on forbidden axiom' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+' rejected by pointer IR audit')
 prefix='''import StdUnsafeCellCorrespondence
namespace StdUnsafeCellPointer
set_option linter.unusedSimpArgs false
def controlPointer : Pointer := ⟨⟨0, 0⟩, 7⟩
def controlHeap : Heap := fun _ => none
-- No identity contract: first cast may change address, second cast may fail.
def shiftedAPI : API := ⟨Outcome.returned, fun kind p h => match kind with
  | .cellToScalar => .returned ⟨⟨p.address.allocation + 1, p.address.offset⟩, p.tag⟩ h
  | .constToMutable => .returned p h⟩
def failingAPI : API := ⟨Outcome.returned, fun kind p h => match kind with
  | .cellToScalar => .returned p h
  | .constToMutable => .fault .externalFailure h⟩
'''
 for label,claim,accepted in [
  ('shift_retained','execute shiftedAPI actualUnsafeCellGet (initial controlPointer) controlHeap = .returned ⟨⟨1, 0⟩, 7⟩ controlHeap',True),
  ('failure_retained','execute failingAPI actualUnsafeCellGet (initial controlPointer) controlHeap = .fault .externalFailure controlHeap',True),
  ('identity_without_contract_false','execute shiftedAPI actualUnsafeCellGet (initial controlPointer) controlHeap = .returned controlPointer controlHeap',False),
  ('success_without_contract_false','execute failingAPI actualUnsafeCellGet (initial controlPointer) controlHeap = .returned controlPointer controlHeap',False)]:
  f=pathlib.Path(tmp)/f'{label}.lean'
  f.write_text(prefix+'example : '+claim+' := by simp [execute, actualUnsafeCellGet, initial, put, bind, shiftedAPI, failingAPI, controlPointer]\nend StdUnsafeCellPointer\n')
  r=subprocess.run(['lake','env','lean','-DwarningAsError=true',str(f)],capture_output=True,text=True)
  if accepted:assert r.returncode==0,r.stdout+r.stderr
  else:assert r.returncode!=0 and 'unsolved goals' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+(' positive control accepted' if accepted else ' false claim rejected'))
print('Four pointer IR theorems checked; zero physical Rust/Cell/layout/provenance refinement theorems.')
NEG
