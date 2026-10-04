#!/usr/bin/env bash
# Restricted actual-MIR sequence, conditional call contracts, not whole Rust/IC equivalence.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
python3 - <<'HASH'
import hashlib,json,pathlib
p=json.load(open('proofs/external/release-timestamp-cfg-profile.json'))
for f,h in p['sha256'].items():
 assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
HASH
python3 scripts/extract_release_timestamp_cfg.py --check
cd proofs/extraction/operator-lean
lake build ReleaseTimestampCFGAudit
lake env lean -DwarningAsError=true ReleaseTimestampCFG.lean
lake env lean -DwarningAsError=true ReleaseTimestampCFGGenerated.lean
lake env lean -DwarningAsError=true ReleaseTimestampCFGCorrespondence.lean
lake env lean -DwarningAsError=true ReleaseTimestampCFGAudit.lean
lake env leanchecker ReleaseTimestampCFGCorrespondence
python3 - <<'NEG'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('ReleaseTimestampCFGAudit.lean').read_text().split('open Lean in',1)
with tempfile.TemporaryDirectory(prefix='release-timestamp-negative-',dir='.lake') as tmp:
 for label,mutation in {
  'axiom':'axiom bad : False\ntheorem negative : False := bad',
  'sorry':'theorem negative : False := by sorry',
  'native':'theorem negative : (1 : Nat) = 1 := by native_decide'}.items():
  f=pathlib.Path(tmp)/f'{label}.lean'
  f.write_text(head+'import Std.Tactic\nnamespace ReleaseTimestampCFG\n'+mutation+'\nend ReleaseTimestampCFG\nopen Lean in'+body)
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'depends on forbidden axiom' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+' rejected by restricted CFG audit')
 f=pathlib.Path(tmp)/'invalid_local.lean'
 f.write_text('import ReleaseTimestampCFGCorrespondence\nopen ReleaseTimestampSequence\nexample (d : Duration) : execute modelCalls 0 d [.narrow 6 99, .returnTimestamp 6] [] = .returned 0 := by rfl\n')
 r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
 assert r.returncode!=0 and 'rfl' in r.stdout+r.stderr,r.stdout+r.stderr
 print('uninitialized-local success shortcut rejected by Lean')
 f=pathlib.Path(tmp)/'skipped_block.lean'
 f.write_text('import ReleaseTimestampCFGCorrespondence\nopen ReleaseTimestampCFG ReleaseTimestampSequence\ndef skipped : Graph := fun pc => if pc = 0 then some ⟨[.input 4, .asNanos 7], some 2⟩ else releaseAddCFG pc\nexample : lower 5 skipped 0 = some releaseAdd := by rfl\n')
 r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
 assert r.returncode!=0 and 'rfl' in r.stdout+r.stderr,r.stdout+r.stderr
 print('skipped conversion block shortcut rejected by Lean')
 f=pathlib.Path(tmp)/'insufficient_fuel.lean'
 f.write_text('import ReleaseTimestampCFGCorrespondence\nopen ReleaseTimestampCFG ReleaseTimestampSequence\nexample : lower 4 releaseAddCFG 0 = some releaseAdd := by rfl\n')
 r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
 assert r.returncode!=0 and 'rfl' in r.stdout+r.stderr,r.stdout+r.stderr
 print('insufficient fuel success shortcut rejected by Lean')
NEG
