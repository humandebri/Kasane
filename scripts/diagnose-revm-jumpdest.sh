#!/usr/bin/env bash
# Three-route actual-JUMPDEST extraction diagnostic; never an instruction proof gate.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
python3 - <<'HASH'
import hashlib,json,pathlib
p=json.load(open('proofs/extraction/tool-patches/revm-jumpdest-profile.json'))
for f,h in p['sha256'].items():
 assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
HASH
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
python3 - <<'RUN'
import json,os,pathlib,subprocess,tempfile
from importlib.util import spec_from_file_location,module_from_spec
spec=spec_from_file_location('jumpdest_audit','scripts/audit_jumpdest_llbc.py')
m=module_from_spec(spec);spec.loader.exec_module(m)
root=pathlib.Path.cwd()
profile=json.load(open('proofs/extraction/tool-patches/revm-jumpdest-profile.json'))
with tempfile.TemporaryDirectory(prefix='kasane-jumpdest-diagnostic-') as tmp:
 tmp=pathlib.Path(tmp)
 for mode in ['generic','mono','partial']:
  mono=mode=='mono'
  dest=tmp/(mode+'.llbc')
  env=os.environ.copy()
  env['CARGO_TARGET_DIR']=str(root/'.local/proof-tools'/('revm-instruction-target' if mono else 'revm-root-target'))
  args=['.local/proof-tools/charon-gat-candidate/charon','cargo','--preset=aeneas','--mir','promoted','--error-on-warnings',
     '--include','revm_interpreter::instructions::control::jumpdest','--opaque','revm_interpreter::interpreter::Interpreter','--dest-file',str(dest)]
  if mode=='partial':args+=['--monomorphize-mut=all']
  if mono:args+=['--monomorphize','--start-from','kasane_revm_instruction_probe::opcode_jumpdest']
  else:args+=['--start-from','revm_interpreter::instructions::control::jumpdest']
  args+=['--','--manifest-path',('proofs/extraction/revm-instruction-rust/Cargo.toml' if mono else 'crates/evm-core/Cargo.toml'),'--lib','--locked']
  proc=subprocess.run(args,env=env,capture_output=True,text=True,timeout=180)
  assert proc.returncode==0,proc.stdout+proc.stderr
  data=json.loads(dest.read_text())
  observation=m.audit(data,mono)
  assert observation==profile['observations'][mode],observation
  m.negative_controls(data,mono)
  out=tmp/(mode+'-lean');out.mkdir()
  args=['.local/proof-tools/aeneas-source/src/_build/default/main.exe','-backend','lean','-dest',str(out),'-namespace','RevmJumpdestDiagnostic','-use-lean-modules','false','-abort-on-error','-warnings-as-errors',str(dest)]
  proc=subprocess.run(args,env=env,capture_output=True,text=True,timeout=180)
  log=proc.stdout+proc.stderr
  assert proc.returncode!=0,log
  if mono:
   assert 'TypesAnalysis.ml, line 229' in log and 'instruction_context.rs' in log and 'Internal error' in log,log
  else:
   assert 'Found an associated type in a trait declaration' in log and 'MemoryTr' in log and 'SymbolicToPure.ml, line 224' in log,log
  assert not list(out.rglob('*.lean')) and not list(out.rglob('*.olean')),'Unexpected accepted/incomplete output'
  print(mode+': fresh actual body/input types retained; known strict rejection; no Lean artifact')
print('Three-route actual JUMPDEST diagnostic passed; nineteen copied-witness changes rejected. No EVM instruction theorem.')
RUN
