#!/usr/bin/env bash
# Actual complete main/unwind/drop protocol audit; no Rust/heap/drop theorem.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/revm-root-target"
python3 - <<'RUN'
import hashlib,json,pathlib,subprocess,sys,tempfile
sys.path.insert(0,'scripts')
from audit_std_cell_replace_llbc import inspect,negative_controls
from extract_std_cell_get_state import lower as lower_get,render as render_get
from extract_std_mem_replace_state import lower as lower_mem,render as render_mem
from extract_std_unsafe_cell_pointer import lower as lower_pointer,render as render_pointer
profile=json.load(open('proofs/extraction/tool-patches/std-cell-replace-protocol-profile.json'))
for mapping in [profile['sha256'],profile['base_source_sha256']]:
 for f,h in mapping.items():assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
with tempfile.TemporaryDirectory(prefix='kasane-std-cell-replace-') as tmp:
 dest=pathlib.Path(tmp)/'cell.llbc'
 args=['.local/proof-tools/charon-gat-candidate/charon','cargo','--preset=aeneas','--mir','promoted','--error-on-warnings','--start-from','core::cell::BorrowRef::new']
 for name in ['core::cell::_::new','core::cell::_::get','core::cell::_::replace','core::cell::BorrowRef','core::cell::Cell','core::cell::UnsafeCell','core::cell::is_reading','core::cell::UNUSED','core::mem::replace','core::ptr::read','core::ptr::write']:args+=['--include',name]
 args+=['--dest-file',str(dest),'--','--manifest-path','crates/evm-core/Cargo.toml','--lib','--locked']
 r=subprocess.run(args,capture_output=True,text=True,timeout=180);assert r.returncode==0,r.stdout+r.stderr
 data=json.loads(dest.read_text());assert inspect(data)==profile['observation'];negative_controls(data)
 for lower,render,name in [(lower_get,render_get,'StdCellGetGenerated'),(lower_mem,render_mem,'StdMemReplaceGenerated'),(lower_pointer,render_pointer,'StdUnsafeCellGenerated')]:
  assert render(lower(data))==pathlib.Path('proofs/extraction/u256-lean/'+name+'.lean').read_text(),'Existing caller/callee IR drift: '+name
print('Fresh actual Cell::replace 28 main/20 nested unwind statements audited; eleven copied ownership/drop/call changes rejected; three existing dependency IR byte matches passed.')
print('No new whole Cell::replace IR or Rust/heap/drop refinement theorem accepted; arbitrary drop effects and resume/terminate behavior still require semantics.')
RUN
