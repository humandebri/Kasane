#!/usr/bin/env bash
# Restricted actual-MIR sequence, conditional call contracts, not whole Rust/IC equivalence.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
python3 - <<'HASH'
import hashlib,json,pathlib
p=json.load(open('proofs/external/release-timestamp-sequence-profile.json'))
for f,h in p['sha256'].items():
 assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
HASH
python3 scripts/extract_release_timestamp_sequence.py --check
cd proofs/extraction/operator-lean
lake build ReleaseTimestampAudit
lake env lean -DwarningAsError=true ReleaseTimestampSequence.lean
lake env lean -DwarningAsError=true ReleaseTimestampGenerated.lean
lake env lean -DwarningAsError=true ReleaseTimestampCorrespondence.lean
lake env lean -DwarningAsError=true ReleaseTimestampAudit.lean
lake env leanchecker ReleaseTimestampCorrespondence
python3 - <<'NEG'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('ReleaseTimestampAudit.lean').read_text().split('open Lean in',1)
with tempfile.TemporaryDirectory(prefix='release-timestamp-negative-',dir='.lake') as tmp:
 for label,mutation in {
  'axiom':'axiom bad : False\ntheorem negative : False := bad',
  'sorry':'theorem negative : False := by sorry',
  'native':'theorem negative : (1 : Nat) = 1 := by native_decide'}.items():
  f=pathlib.Path(tmp)/f'{label}.lean'
  f.write_text(head+'import Std.Tactic\nnamespace ReleaseTimestampSequence\n'+mutation+'\nend ReleaseTimestampSequence\nopen Lean in'+body)
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'depends on forbidden axiom' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+' rejected by conditional sequence audit')
 f=pathlib.Path(tmp)/'invalid_local.lean'
 f.write_text('import ReleaseTimestampCorrespondence\nopen ReleaseTimestampSequence\nexample (d : Duration) : execute modelCalls 0 d [.narrow 6 99, .returnTimestamp 6] [] = .returned 0 := by rfl\n')
 r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
 assert r.returncode!=0 and 'rfl' in r.stdout+r.stderr,r.stdout+r.stderr
 print('uninitialized-local success shortcut rejected by Lean')
NEG
