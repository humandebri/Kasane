#!/usr/bin/env bash
# Actual MemoryTr slice_len diagnostic; no generated theorem or whole-memory proof.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/revm-root-target"
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
python3 - <<'RUN'
import hashlib,json,pathlib,subprocess,tempfile,sys
sys.path.insert(0,'scripts')
from audit_memory_slice_len_llbc import inspect,negative_controls
profile=json.load(open('proofs/extraction/tool-patches/memory-slice-len-profile.json'))
for mapping in [profile['sha256'],profile['base_source_sha256']]:
 for f,h in mapping.items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
with tempfile.TemporaryDirectory(prefix='kasane-memory-slice-len-') as tmp:
 tmp=pathlib.Path(tmp)
 for transparent in [False,True]:
  dest=tmp/('ref.llbc' if transparent else 'opaque.llbc')
  args=['.local/proof-tools/charon-gat-candidate/charon','cargo','--preset=aeneas','--mir','promoted','--remove-unused-clauses','--error-on-warnings','--start-from','revm_interpreter::interpreter_types::MemoryTr::slice_len','--include','revm_interpreter::interpreter_types::MemoryTr::slice_len']
  if transparent:args+=['--include','core::cell::Ref']
  args+=['--dest-file',str(dest),'--','--manifest-path','crates/evm-core/Cargo.toml','--lib','--locked']
  p=subprocess.run(args,capture_output=True,text=True,timeout=180);assert p.returncode==0,p.stdout+p.stderr
  data=json.loads(dest.read_text());assert inspect(data,transparent)==profile['observations'][str(transparent).lower()];negative_controls(data,transparent)
  p=subprocess.run(['.local/proof-tools/aeneas-gat-source/aeneas','-borrow-check','-abort-on-error','-warnings-as-errors',str(dest)],capture_output=True,text=True,timeout=180)
  assert p.returncode==0 and 'Crate successfully borrow-checked' in p.stdout+p.stderr,p.stdout+p.stderr
  out=tmp/('ref-lean' if transparent else 'opaque-lean');out.mkdir()
  p=subprocess.run(['.local/proof-tools/aeneas-gat-source/aeneas','-backend','lean','-dest',str(out),'-namespace','RevmMemorySliceLen','-use-lean-modules','false','-abort-on-error','-warnings-as-errors',str(dest)],capture_output=True,text=True,timeout=180)
  log=p.stdout+p.stderr
  assert p.returncode!=0 and 'Found an associated type in a trait declaration' in log and 'MemoryTr' in log,log
  assert not list(out.rglob('*.lean')) and not list(out.rglob('*.olean'))
print('Fresh actual slice_len: opaque and transparent Ref routes both borrow-check; same receiver/result lifetime, checked addition and slice trait call audited; twelve witness mutations rejected; both strict Lean routes reject GAT with no output; no memory theorem.')
RUN
