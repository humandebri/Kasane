#!/usr/bin/env bash
# Unadopted borrow-only callback candidate; no Rust or generated Lean proof.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/revm-root-target"
python3 - <<'RUN'
import hashlib,json,pathlib,subprocess,tempfile,sys
sys.path.insert(0,'scripts')
from audit_revm_step_llbc import inspect,negative_controls
root=pathlib.Path.cwd();profile=json.load(open('proofs/extraction/tool-patches/dynamic-borrow-profile.json'))
for mapping in [profile['sha256'],profile['base_source_sha256']]:
 for f,h in mapping.items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
baseline=json.load(open('proofs/extraction/tool-patches/revm-step-dispatch-profile.json'))
with tempfile.TemporaryDirectory(prefix='kasane-dynamic-borrow-') as tmp:
 tmp=pathlib.Path(tmp);dest=tmp/'step.llbc'
 args=['.local/proof-tools/charon-gat-candidate/charon','cargo','--preset=aeneas','--mir','promoted','--remove-unused-clauses','--error-on-warnings','--start-from','revm_interpreter::interpreter::Interpreter::step']
 for name in ['revm_interpreter::interpreter::_::step','revm_interpreter::interpreter::_::halt_oog','revm_interpreter::interpreter::_::halt','revm_interpreter::interpreter::Interpreter','revm_interpreter::gas::Gas','revm_interpreter::gas::_::record_cost_unsafe','revm_interpreter::instructions::Instruction','revm_interpreter::instructions::_::static_gas','revm_interpreter::instructions::_::execute']:
  args+=['--include',name]
 args+=['--dest-file',str(dest),'--','--manifest-path','crates/evm-core/Cargo.toml','--lib','--locked']
 proc=subprocess.run(args,capture_output=True,text=True,timeout=180)
 assert proc.returncode==0,proc.stdout+proc.stderr
 data=json.loads(dest.read_text());assert inspect(data)==baseline['observation'];negative_controls(data)
 proc=subprocess.run(['.local/proof-tools/aeneas-dynamic-borrow/check',str(dest)],capture_output=True,text=True,timeout=180)
 assert proc.returncode==0 and 'six unsupported signature controls rejected' in proc.stdout+proc.stderr,proc.stdout+proc.stderr
 proc=subprocess.run(['.local/proof-tools/aeneas-dynamic-borrow/aeneas','-borrow-check','-abort-on-error','-warnings-as-errors',str(dest)],capture_output=True,text=True,timeout=180)
 assert proc.returncode==0 and 'Crate successfully borrow-checked' in proc.stdout+proc.stderr,proc.stdout+proc.stderr
 out=tmp/'lean';out.mkdir()
 proc=subprocess.run(['.local/proof-tools/aeneas-dynamic-borrow/aeneas','-backend','lean','-dest',str(out),'-namespace','RevmDynamicBorrow','-use-lean-modules','false','-abort-on-error','-warnings-as-errors',str(dest)],capture_output=True,text=True,timeout=180)
 log=proc.stdout+proc.stderr
 assert proc.returncode!=0 and 'Unsupported open or non-Rust function pointer signature' in log and 'instructions.rs' in log,log
 assert not list(out.rglob('*.lean')) and not list(out.rglob('*.olean'))
print('Fresh actual six-body unit borrow-checked in unadopted candidate; nine LLBC and six native signature controls rejected; strict Lean generation still rejected; no execution theorem.')
RUN
