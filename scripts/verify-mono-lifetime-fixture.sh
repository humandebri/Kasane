#!/usr/bin/env bash
# Lifetime-only frontend regression; no EVM or Rust memory equivalence claim.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
python3 - <<'HASH'
import hashlib,json,pathlib
p=json.load(open('proofs/extraction/tool-patches/mono-lifetime-profile.json'))
for f,h in p['sha256'].items():
 assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
HASH
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/mono-lifetime-target"
python3 - <<'EXTRACT'
import json,os,pathlib,subprocess,tempfile
from importlib.util import spec_from_file_location,module_from_spec
spec=spec_from_file_location('lifetime_audit','scripts/audit_mono_lifetime_llbc.py')
m=module_from_spec(spec);spec.loader.exec_module(m)
root=pathlib.Path.cwd()
with tempfile.TemporaryDirectory(prefix='kasane-mono-lifetime-') as tmp:
 tmp=pathlib.Path(tmp)
 for mono in [False,True]:
  dest=tmp/('mono.llbc' if mono else 'generic.llbc')
  args=['.local/proof-tools/charon-gat-candidate/charon','cargo','--preset=aeneas','--mir','promoted','--error-on-warnings','--dest-file',str(dest)]
  if mono:args+=['--monomorphize']
  args+=['--','--manifest-path','proofs/extraction/mono-lifetime-rust/Cargo.toml','--lib','--locked']
  proc=subprocess.run(args,capture_output=True,text=True,timeout=180)
  assert proc.returncode==0,proc.stdout+proc.stderr
  data=json.loads(dest.read_text());print(json.dumps(m.audit(data,mono)))
  if not mono:m.negative_controls(data)
  out=tmp/('mono-lean' if mono else 'generic-lean');out.mkdir()
  args=['.local/proof-tools/aeneas-source/src/_build/default/main.exe','-backend','lean','-dest',str(out),'-namespace',('MonoLifetimeMono' if mono else 'MonoLifetimeGeneric'),'-use-lean-modules','false','-abort-on-error','-warnings-as-errors',str(dest)]
  proc=subprocess.run(args,capture_output=True,text=True,timeout=180)
  log=proc.stdout+proc.stderr
  if mono:
   assert proc.returncode!=0 and 'TypesAnalysis.ml, line 229' in log,log
   assert not list(out.rglob('*.lean')) and not list(out.rglob('*.olean'))
  else:
   assert proc.returncode==0,log
   assert (out/'Generic.lean').read_bytes()==(root/'proofs/extraction/u256-lean/MonoLifetimeGenerated.lean').read_bytes(),'Generated Lean drift'
 print('Fresh generic translation byte-equal; four lifetime mutations rejected; full monomorphization strictly rejected with no Lean output.')
EXTRACT
cd proofs/extraction/u256-lean
lake build MonoLifetimeAudit
for source in MonoLifetimeGenerated MonoLifetimeCorrespondence MonoLifetimeAudit; do
 lake env lean -DwarningAsError=true "$source.lean"
done
lake env leanchecker MonoLifetimeCorrespondence
python3 - <<'NEG'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('MonoLifetimeAudit.lean').read_text().split('open Lean in',1)
with tempfile.TemporaryDirectory(prefix='mono-lifetime-negative-',dir='.lake') as tmp:
 for label,mutation in {
  'axiom':'axiom bad : False\ntheorem negative : False := bad',
  'sorry':'theorem negative : False := by sorry',
  'native':'theorem negative : (1 : Nat) = 1 := by native_decide'}.items():
  f=pathlib.Path(tmp)/f'{label}.lean'
  f.write_text(head+'import Std.Tactic\nnamespace MonoLifetimeCorrespondence\n'+mutation+'\nend MonoLifetimeCorrespondence\nopen Lean in'+body)
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'depends on forbidden axiom' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+' rejected by audit')
 for label,claim in {
  'wrong_left_update':'split_left context = .ok (context.left, (fun value => { context with right := value }), context)',
  'wrong_right_update':'split_right context = .ok (context.right, context, (fun value => { context with left := value }))'}.items():
  f=pathlib.Path(tmp)/f'{label}.lean'
  f.write_text('import MonoLifetimeCorrespondence\nopen MonoLifetimeGeneric\nexample (context : Split) : '+claim+' := by rfl\n')
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'rfl' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+' false callback correspondence rejected')
NEG
