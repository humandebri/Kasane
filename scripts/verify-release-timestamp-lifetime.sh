#!/usr/bin/env bash
# Restricted actual-MIR sequence, conditional call contracts, not whole Rust/IC equivalence.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
python3 - <<'HASH'
import hashlib,json,pathlib
p=json.load(open('proofs/external/release-timestamp-lifetime-profile.json'))
for f,h in p['sha256'].items():
 assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
HASH
python3 scripts/extract_release_timestamp_lifetime.py --check
cd proofs/extraction/operator-lean
lake build ReleaseTimestampLifetimeAudit
lake env lean -DwarningAsError=true ReleaseTimestampLocalLifetime.lean
lake env lean -DwarningAsError=true ReleaseTimestampLifetimeGenerated.lean
lake env lean -DwarningAsError=true ReleaseTimestampLifetimeCorrespondence.lean
lake env lean -DwarningAsError=true ReleaseTimestampLifetimeAudit.lean
lake env leanchecker ReleaseTimestampLifetimeCorrespondence
python3 - <<'NEG'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('ReleaseTimestampLifetimeAudit.lean').read_text().split('open Lean in',1)
with tempfile.TemporaryDirectory(prefix='release-timestamp-negative-',dir='.lake') as tmp:
 for label,mutation in {
  'axiom':'axiom bad : False\ntheorem negative : False := bad',
  'sorry':'theorem negative : False := by sorry',
  'native':'theorem negative : (1 : Nat) = 1 := by native_decide'}.items():
  f=pathlib.Path(tmp)/f'{label}.lean'
  f.write_text(head+'import Std.Tactic\nnamespace ReleaseTimestampLocalLifetime\n'+mutation+'\nend ReleaseTimestampLocalLifetime\nopen Lean in'+body)
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'depends on forbidden axiom' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+' rejected by local lifetime audit')
 for label,expr in {
  'missing_storage_live':'addEvents.filter (fun e => e != Event.live 8)',
  'uninitialized_call_argument':'addEvents.map (fun e => if e = Event.call [7] 6 then Event.call [9] 6 else e)',
  'dead_return_local':'addEvents.map (fun e => if e = Event.ret 0 then Event.ret 4 else e)'}.items():
  f=pathlib.Path(tmp)/f'{label}.lean'
  f.write_text('import ReleaseTimestampLifetimeCorrespondence\nopen ReleaseTimestampLocalLifetime\nexample : check true ('+expr+') initial = some finalState := by decide\n')
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'decide' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+' false acceptance rejected by Lean')
NEG
