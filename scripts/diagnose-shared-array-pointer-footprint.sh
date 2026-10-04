#!/usr/bin/env bash
# Kernel-checked footprint MODEL only; does not enable Rust pointer extraction.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
python3 - <<'PYHASH'
import json,hashlib,pathlib
p=json.load(open('proofs/extraction/tool-patches/shared-array-pointer-footprint-profile.json'))
for f,h in p['sha256'].items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
previous=json.load(open('proofs/extraction/tool-patches/array-nonnull-pure-profile.json'))
for key in ['sha256','base_source_sha256']:
 for f,h in previous[key].items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
PYHASH
cd proofs/extraction/operator-lean
lake build SharedArrayPointerFootprintAudit
lake env lean -DwarningAsError=true SharedArrayPointerFootprint.lean
lake env lean -DwarningAsError=true SharedArrayPointerFootprintAudit.lean
lake env leanchecker SharedArrayPointerFootprint
python3 - <<'PYNEG'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('SharedArrayPointerFootprintAudit.lean').read_text().split('open Lean in',1)
mutations={'axiom':'axiom bad : False\ntheorem negative : False := bad',
 'sorry':'theorem negative : False := by sorry',
 'native':'theorem negative : (1 : Nat) = 1 := by native_decide'}
with tempfile.TemporaryDirectory(prefix='pointer-footprint-negative-',dir='.lake') as tmp:
 for name,mutation in mutations.items():
  f=pathlib.Path(tmp)/f'{name}.lean'
  f.write_text(head+'import Std.Tactic\nnamespace SharedArrayPointerFootprint\n'+mutation+'\nend SharedArrayPointerFootprint\nopen Lean in'+body)
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'depends on forbidden axiom' in r.stdout+r.stderr,r.stdout+r.stderr
  print(name+' rejected by pointer footprint model audit')
 f=pathlib.Path(tmp)/'false_bounds.lean'
 f.write_text('import SharedArrayPointerFootprint\nopen SharedArrayPointerFootprint\nexample : ∀ (p : Pointer) (bytes : Nat), p.address ≠ 0 → ReadFootprint p bytes := by\n intro p bytes nonnull\n exact ⟨nonnull, Or.inl rfl⟩\n')
 r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
 assert r.returncode!=0 and 'type mismatch' in r.stdout+r.stderr,r.stdout+r.stderr
 print('non-null-only positive read shortcut rejected by Lean')
PYNEG
