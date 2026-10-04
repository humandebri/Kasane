#!/usr/bin/env bash
# Actual caller/callee scalar IR composition; physical Rust/Cell refinement unproved.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/revm-root-target"
python3 - <<'RUN'
import hashlib,json,pathlib,subprocess,sys,tempfile
sys.path.insert(0,'scripts')
from extract_std_cell_get_state import lower,render,negative_controls
from extract_std_unsafe_cell_pointer import lower as lower_unsafe,render as render_unsafe
profile=json.load(open('proofs/extraction/tool-patches/std-cell-get-state-profile.json'))
for mapping in [profile['sha256'],profile['base_source_sha256']]:
 for f,h in mapping.items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
with tempfile.TemporaryDirectory(prefix='kasane-std-cell-get-state-') as tmp:
 dest=pathlib.Path(tmp)/'cell.llbc'
 args=['.local/proof-tools/charon-gat-candidate/charon','cargo','--preset=aeneas','--mir','promoted','--error-on-warnings','--start-from','core::cell::BorrowRef::new']
 for name in ['core::cell::_::new','core::cell::_::get','core::cell::_::replace','core::cell::BorrowRef','core::cell::Cell','core::cell::UnsafeCell','core::cell::is_reading','core::cell::UNUSED','core::mem::replace','core::ptr::read','core::ptr::write']:args+=['--include',name]
 args+=['--dest-file',str(dest),'--','--manifest-path','crates/evm-core/Cargo.toml','--lib','--locked']
 r=subprocess.run(args,capture_output=True,text=True,timeout=180);assert r.returncode==0,r.stdout+r.stderr
 data=json.loads(dest.read_text());ops=lower(data);negative_controls(data)
 assert ops==profile['operations']
 assert render(ops)==pathlib.Path('proofs/extraction/u256-lean/StdCellGetGenerated.lean').read_text(),'Actual caller IR drift'
 assert render_unsafe(lower_unsafe(data))==pathlib.Path('proofs/extraction/u256-lean/StdUnsafeCellGenerated.lean').read_text(),'Actual callee IR drift'
print('Fresh actual Cell get ten main statements, two unwind statements and callee/layout matched; nine copied witness changes rejected.')
RUN
cd proofs/extraction/u256-lean
for source in StdMemReplaceState StdUnsafeCellPointer StdUnsafeCellGenerated StdUnsafeCellCorrespondence StdUnsafeCellAudit StdCellGetState StdCellGetGenerated StdCellGetCorrespondence StdCellGetAudit; do
 lake env lean -DwarningAsError=true -o ".lake/build/lib/lean/$source.olean" "$source.lean"
done
lake env leanchecker StdCellGetCorrespondence
python3 - <<'NEG'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('StdCellGetAudit.lean').read_text().split('open Lean in',1)
with tempfile.TemporaryDirectory(prefix='cell-get-negative-',dir='.lake') as tmp:
 for label,mutation in {
  'axiom':'axiom bad : False\ntheorem negative : False := bad',
  'sorry':'theorem negative : False := by sorry',
  'native':'theorem negative : (1 : Nat) = 1 := by native_decide'}.items():
  f=pathlib.Path(tmp)/f'{label}.lean';f.write_text(head+'import Std.Tactic\nnamespace StdCellGetState\n'+mutation+'\nend StdCellGetState\nopen Lean in'+body)
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'depends on forbidden axiom' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+' rejected by composed Cell get IR audit')
 prefix='''import StdCellGetCorrespondence
open Aeneas Aeneas.Std
namespace StdCellGetState
set_option linter.unusedSimpArgs false
def controlPointer : Pointer := ⟨⟨0, 0⟩, 7⟩
def controlHeap : Heap := fun a => if a.allocation = 1 then some (9#isize) else some (0#isize)
def controlRights : Permissions := ⟨fun _ => true, fun _ => true⟩
def shiftedProjection (p : Pointer) (h : Heap) : PointerOutcome :=
  .returned ⟨⟨p.address.allocation + 1, p.address.offset⟩, p.tag⟩ h
def failedProjection (p : Pointer) (h : Heap) : PointerOutcome :=
  .fault .externalFailure (StdMemReplaceState.writeHeap h p.address (7#isize))
def valueObservation : Option Scalar :=
  match execute (linkedAPI shiftedProjection StdUnsafeCellPointer.identityAPI controlRights)
    actualCellGet (initial controlPointer) controlHeap with
  | .returned v _ => some v
  | .fault _ _ => none
def faultHeapObservation : Option Scalar :=
  match execute (linkedAPI failedProjection StdUnsafeCellPointer.identityAPI controlRights)
    actualCellGet (initial controlPointer) controlHeap with
  | .returned _ h => h controlPointer.address
  | .fault _ h => h controlPointer.address
'''
 for label,claim,accepted in [
  ('shift_reads_new_address','valueObservation = some (9#isize)',True),
  ('failure_heap_retained','faultHeapObservation = some (7#isize)',True),
  ('original_address_false','valueObservation = some (0#isize)',False),
  ('failure_heap_reset_false','faultHeapObservation = some (0#isize)',False)]:
  f=pathlib.Path(tmp)/f'{label}.lean'
  f.write_text(prefix+'example : '+claim+' := by simp [valueObservation, faultHeapObservation, execute, actualCellGet, initial, put, bindPointer, linkedAPI, readHeap, shiftedProjection, failedProjection, controlPointer, controlHeap, controlRights, StdMemReplaceState.writeHeap, StdUnsafeCellPointer.identity_model, IScalar.eq_equiv]\nend StdCellGetState\n')
  r=subprocess.run(['lake','env','lean','-DwarningAsError=true',str(f)],capture_output=True,text=True)
  if accepted:assert r.returncode==0,r.stdout+r.stderr
  else:assert r.returncode!=0 and 'unsolved goals' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+(' positive control accepted' if accepted else ' false state observation rejected'))
print('Five composed Cell get IR theorems and one helper checked; zero physical Rust/Cell/ref/provenance refinement theorems.')
NEG
