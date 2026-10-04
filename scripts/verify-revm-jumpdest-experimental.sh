#!/usr/bin/env bash
# Actual generated leaf body only; trait erasure and Rust/EVM refinement unproved.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
python3 - <<'HASH'
import hashlib,json,pathlib
p=json.load(open('proofs/extraction/tool-patches/revm-jumpdest-transparent-profile.json'))
for f,h in p['sha256'].items():
 assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
HASH
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/revm-root-target"
python3 - <<'EXTRACT'
import json,pathlib,subprocess,tempfile,sys
sys.path.insert(0,'scripts')
from audit_jumpdest_transparent_llbc import transparent_audit,transparent_negative_controls
root=pathlib.Path.cwd()
profile=json.load(open('proofs/extraction/tool-patches/revm-jumpdest-transparent-profile.json'))
with tempfile.TemporaryDirectory(prefix='kasane-jumpdest-transparent-') as tmp:
 tmp=pathlib.Path(tmp);dest=tmp/'jumpdest.llbc'
 args=['.local/proof-tools/charon-gat-candidate/charon','cargo','--preset=aeneas','--mir','promoted','--remove-unused-clauses','--error-on-warnings','--start-from','revm_interpreter::instructions::control::jumpdest','--include','revm_interpreter::instructions::control::jumpdest','--include','revm_interpreter::interpreter::Interpreter','--include','revm_interpreter::gas::Gas','--dest-file',str(dest),'--','--manifest-path','crates/evm-core/Cargo.toml','--lib','--locked']
 proc=subprocess.run(args,capture_output=True,text=True,timeout=180)
 assert proc.returncode==0,proc.stdout+proc.stderr
 data=json.loads(dest.read_text())
 assert transparent_audit(data)==profile['observation']
 transparent_negative_controls(data)
 out=tmp/'lean';out.mkdir()
 args=['.local/proof-tools/aeneas-source/src/_build/default/main.exe','-backend','lean','-dest',str(out),'-namespace','RevmJumpdestTransparent','-use-lean-modules','false','-abort-on-error','-warnings-as-errors',str(dest)]
 proc=subprocess.run(args,capture_output=True,text=True,timeout=180)
 assert proc.returncode==0,proc.stdout+proc.stderr
 assert (out/'Jumpdest.lean').read_bytes()==(root/'proofs/extraction/u256-lean/RevmJumpdestGenerated.lean').read_bytes(),'Generated Lean drift'
 print('Fresh actual-body/transparent-type extraction byte-equal; twelve LLBC changes rejected.')
EXTRACT
cd proofs/extraction/u256-lean
lake build RevmJumpdestAudit
for source in RevmJumpdestGenerated RevmJumpdestCorrespondence RevmJumpdestAudit; do
 lake env lean -DwarningAsError=true "$source.lean"
done
lake env leanchecker RevmJumpdestCorrespondence
python3 - <<'NEG'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('RevmJumpdestAudit.lean').read_text().split('open Lean in',1)
with tempfile.TemporaryDirectory(prefix='jumpdest-negative-',dir='.lake') as tmp:
 for label,mutation in {
  'axiom':'axiom bad : False\ntheorem negative : False := bad',
  'sorry':'theorem negative : False := by sorry',
  'native':'theorem negative : (1 : Nat) = 1 := by native_decide'}.items():
  f=pathlib.Path(tmp)/f'{label}.lean'
  f.write_text(head+'import Std.Tactic\nnamespace RevmJumpdestCorrespondence\n'+mutation+'\nend RevmJumpdestCorrespondence\nopen Lean in'+body)
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'depends on forbidden axiom' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+' rejected by audit')
 prefix=pathlib.Path('RevmJumpdestCorrespondence.lean').read_text().split('-- All generated context')[0]
 context='Context (WIRE := WIRE) (H := H) (Stack := Stack) (Memory := Memory) (Bytecode := Bytecode) (ReturnData := ReturnData) (Input := Input) (RuntimeFlag := RuntimeFlag) (Extend := Extend) (Output := Output)'
 for label,claim in {
  'gas_write':'instructions.control.jumpdest context = .ok {context with interpreter := {context.interpreter with gas := {context.interpreter.gas with remaining := context.interpreter.gas.limit}}}',
  'failure':'instructions.control.jumpdest context = .fail .panic'}.items():
  f=pathlib.Path(tmp)/f'{label}.lean'
  f.write_text(prefix+'example (context : '+context+') : '+claim+' := by rfl\nend RevmJumpdestCorrespondence\n')
  r=subprocess.run(['lake','env','lean',str(f)],capture_output=True,text=True)
  assert r.returncode!=0 and 'rfl' in r.stdout+r.stderr,r.stdout+r.stderr
  print(label+' false body claim rejected')
NEG
