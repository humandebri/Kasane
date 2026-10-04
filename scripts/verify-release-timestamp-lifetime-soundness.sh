#!/usr/bin/env bash
# Soundness of the limited local checker; not whole Rust MIR/memory equivalence.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
python3 - <<'HASH'
import hashlib,json,pathlib
p=json.load(open('proofs/external/release-timestamp-lifetime-soundness-profile.json'))
for f,h in p['sha256'].items():
 assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
HASH
python3 scripts/extract_release_timestamp_lifetime.py --check
cd proofs/extraction/operator-lean
lake build ReleaseTimestampLifetimeSoundnessAudit
lake env lean -DwarningAsError=true ReleaseTimestampLifetimeSoundness.lean
lake env lean -DwarningAsError=true ReleaseTimestampLifetimeSoundnessAudit.lean
lake env leanchecker ReleaseTimestampLifetimeSoundness
python3 - <<'NEG'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('ReleaseTimestampLifetimeSoundnessAudit.lean').read_text().split('open Lean in',1)
with tempfile.TemporaryDirectory(prefix='release-lifetime-soundness-negative-',dir='.lake') as tmp:
 for label,mutation in {
  'axiom':'axiom bad : False\ntheorem negative : False := bad',
  'sorry':'theorem negative : False := by sorry',
  'native':'theorem negative : (1 : Nat) = 1 := by native_decide'}.items():
  f=pathlib.Path(tmp)/f'{label}.lean'
  f.write_text(head+'import Std.Tactic\nnamespace ReleaseTimestampLifetimeSoundness\n'+mutation+'\nend ReleaseTimestampLifetimeSoundness\nopen Lean in'+body)
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'depends on forbidden axiom' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+' rejected by soundness audit')
 for label,claim in {
  'uninitialized_read':'ReadsInitialized (.ret 0) initial',
  'unallocated_write':'WritesLive (.call [1] 9) initial'}.items():
  f=pathlib.Path(tmp)/f'{label}.lean'
  f.write_text('import ReleaseTimestampLifetimeSoundness\nopen ReleaseTimestampLocalLifetime ReleaseTimestampLifetimeSoundness\nexample : '+claim+' := by simp [ReadsInitialized, WritesLive, readLocals, writeLocals, initial]\n')
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'unsolved goals' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+' false safety claim rejected by Lean')
NEG
