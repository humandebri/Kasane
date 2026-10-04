#!/usr/bin/env bash
# Conditional scalar heap IR proof; source/extractor/pointer/Rust refinement unproved.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/revm-root-target"
python3 - <<'RUN'
import hashlib,json,pathlib,subprocess,sys,tempfile
sys.path.insert(0,'scripts')
from extract_std_mem_replace_state import lower,render,negative_controls,drift_controls
profile=json.load(open('proofs/extraction/tool-patches/std-mem-replace-state-profile.json'))
for mapping in [profile['sha256'],profile['base_source_sha256']]:
 for f,h in mapping.items(): assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
with tempfile.TemporaryDirectory(prefix='kasane-std-mem-replace-state-') as tmp:
 dest=pathlib.Path(tmp)/'cell.llbc'
 args=['.local/proof-tools/charon-gat-candidate/charon','cargo','--preset=aeneas','--mir','promoted','--error-on-warnings','--start-from','core::cell::BorrowRef::new']
 for name in ['core::cell::_::new','core::cell::_::get','core::cell::_::replace','core::cell::BorrowRef','core::cell::Cell','core::cell::UnsafeCell','core::cell::is_reading','core::cell::UNUSED','core::mem::replace','core::ptr::read','core::ptr::write']:args+=['--include',name]
 args+=['--dest-file',str(dest),'--','--manifest-path','crates/evm-core/Cargo.toml','--lib','--locked']
 r=subprocess.run(args,capture_output=True,text=True,timeout=180);assert r.returncode==0,r.stdout+r.stderr
 data=json.loads(dest.read_text());ops=lower(data)
 assert ops==profile['operations'];negative_controls(data);drift_controls(data)
 assert render(ops)==pathlib.Path('proofs/extraction/u256-lean/StdMemReplaceGenerated.lean').read_text(),'Actual generated IR drift'
print('Fresh actual mem::replace all fifteen statements matched; eight schema changes rejected; two supported operand changes retained.')
RUN
cd proofs/extraction/u256-lean
for source in StdMemReplaceState StdMemReplaceGenerated StdMemReplaceCorrespondence StdMemReplaceAudit; do
 lake env lean -DwarningAsError=true -o ".lake/build/lib/lean/$source.olean" "$source.lean"
done
lake env leanchecker StdMemReplaceCorrespondence
python3 - <<'NEG'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('StdMemReplaceAudit.lean').read_text().split('open Lean in',1)
with tempfile.TemporaryDirectory(prefix='mem-replace-negative-',dir='.lake') as tmp:
 for label,mutation in {
  'axiom':'axiom bad : False\ntheorem negative : False := bad',
  'sorry':'theorem negative : False := by sorry',
  'native':'theorem negative : (1 : Nat) = 1 := by native_decide'}.items():
  f=pathlib.Path(tmp)/f'{label}.lean'
  f.write_text(head+'import Std.Tactic\nnamespace StdMemReplaceState\n'+mutation+'\nend StdMemReplaceState\nopen Lean in'+body)
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'depends on forbidden axiom' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+' rejected by conditional heap IR audit')
 prefix='''import StdMemReplaceCorrespondence
open Aeneas Aeneas.Std
namespace StdMemReplaceState
set_option linter.unusedSimpArgs false
-- The two different tags share an allocation/offset in this finite control.
def controlPointer : Pointer := ⟨⟨0, 0⟩, 7⟩
def controlAlias : Pointer := ⟨⟨0, 0⟩, 8⟩
def controlHeap : Heap := fun _ => some (0#isize)
def controlRights : Permissions := ⟨fun _ => true, fun _ => true⟩
def controlObservation (a : Address) : Option Scalar :=
  match execute controlRights actualMemReplace (initial controlPointer (1#isize)) controlHeap with
  | .returned _ h => h a
  | .fault _ _ => none
'''
 for label,claim,accepted in [
  ('alias_new','controlObservation controlAlias.address = some (1#isize)',True),
  ('other_old','controlObservation ⟨0, 1⟩ = some (0#isize)',True),
  ('alias_old_false','controlObservation controlAlias.address = some (0#isize)',False),
  ('other_changed_false','controlObservation ⟨0, 1⟩ = some (1#isize)',False)]:
  f=pathlib.Path(tmp)/f'{label}.lean';f.write_text(prefix+'example : '+claim+' := by simp [controlObservation, controlPointer, controlAlias, controlHeap, controlRights, execute, actualMemReplace, initial, put, writeHeap, IScalar.eq_equiv]\nend StdMemReplaceState\n')
  r=subprocess.run(['lake','env','lean','-DwarningAsError=true',str(f)],capture_output=True,text=True)
  if accepted:assert r.returncode==0,r.stdout+r.stderr
  else:assert r.returncode!=0 and 'unsolved goals' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+(' positive control accepted' if accepted else ' false heap observation rejected'))
print('Seven conditional scalar heap IR theorems checked; zero full Rust/Cell/provenance refinement theorems.')
NEG
