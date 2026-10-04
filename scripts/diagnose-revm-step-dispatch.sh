#!/usr/bin/env bash
# Actual step extraction diagnostic, not an instruction or dispatch proof gate.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
python3 - <<'HASH'
import hashlib,json,pathlib
p=json.load(open('proofs/extraction/tool-patches/revm-step-dispatch-profile.json'))
for f,h in p['sha256'].items():
 assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
HASH
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/revm-root-target"
python3 - <<'RUN'
import json,pathlib,subprocess,tempfile,sys
sys.path.insert(0,'scripts')
from audit_revm_step_llbc import inspect,negative_controls
root=pathlib.Path.cwd()
p=json.load(open('proofs/extraction/tool-patches/revm-step-dispatch-profile.json'))
with tempfile.TemporaryDirectory(prefix='kasane-step-dispatch-') as tmp:
 tmp=pathlib.Path(tmp);dest=tmp/'step.llbc'
 args=['.local/proof-tools/charon-gat-candidate/charon','cargo','--preset=aeneas','--mir','promoted','--remove-unused-clauses','--error-on-warnings','--start-from','revm_interpreter::interpreter::Interpreter::step']
 for name in ['revm_interpreter::interpreter::_::step','revm_interpreter::interpreter::_::halt_oog','revm_interpreter::interpreter::_::halt','revm_interpreter::interpreter::Interpreter','revm_interpreter::gas::Gas','revm_interpreter::gas::_::record_cost_unsafe','revm_interpreter::instructions::Instruction','revm_interpreter::instructions::_::static_gas','revm_interpreter::instructions::_::execute']:
  args+=['--include',name]
 args+=['--dest-file',str(dest),'--','--manifest-path','crates/evm-core/Cargo.toml','--lib','--locked']
 proc=subprocess.run(args,capture_output=True,text=True,timeout=180)
 assert proc.returncode==0,proc.stdout+proc.stderr
 data=json.loads(dest.read_text());assert inspect(data)==p['observation']
 negative_controls(data)
 out=tmp/'lean';out.mkdir()
 args=['.local/proof-tools/aeneas-source/src/_build/default/main.exe','-backend','lean','-dest',str(out),'-namespace','RevmStepDispatch','-use-lean-modules','false','-abort-on-error','-warnings-as-errors',str(dest)]
 proc=subprocess.run(args,capture_output=True,text=True,timeout=180)
 log=proc.stdout+proc.stderr
 assert proc.returncode!=0 and 'Arrow types are not supported yet' in log and 'SymbolicToPureTypes.ml, line 190' in log and 'instructions.rs' in log,log
 assert not list(out.rglob('*.lean')) and not list(out.rglob('*.olean')),'Unexpected output'
 for mode, route in p['borrow_routes'].items():
  proc=subprocess.run([route['binary'],'-borrow-check','-abort-on-error','-warnings-as-errors',str(dest)],capture_output=True,text=True,timeout=180)
  log=proc.stdout+proc.stderr
  assert proc.returncode!=0 and all(marker in log for marker in route['expected_markers']),mode+': '+log
  print(mode+': actual-body borrow route strictly rejected at recorded stage')
 print('Fresh actual six-function step extraction audited; nine copied-witness changes rejected; type extraction and four borrow routes rejected at recorded stages; no Lean artifact or execution theorem.')
RUN
