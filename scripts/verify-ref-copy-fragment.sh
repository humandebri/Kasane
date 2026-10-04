#!/usr/bin/env bash
# Model-level check only; does not adopt the candidate or prove its Rust meaning.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/../proofs/extraction/operator-lean"
lake build RefCopyFragment RefCopyAudit
lake env lean -DwarningAsError=true RefCopyFragment.lean
lake env lean -DwarningAsError=true RefCopyAudit.lean
lake env leanchecker RefCopyFragment
python3 - <<'PYTEST'
import pathlib,subprocess,tempfile
src=pathlib.Path('RefCopyAudit.lean').read_text()
head,body=src.split('open Lean in',1)
mutations={
 'axiom':'namespace RefCopyFragment\naxiom bad : False\ntheorem negative : False := bad\nend RefCopyFragment\n',
 'sorry':'namespace RefCopyFragment\ntheorem negative : False := by sorry\nend RefCopyFragment\n',
 'native':'namespace RefCopyFragment\ntheorem negative : (1 : Nat) = 1 := by native_decide\nend RefCopyFragment\n'
}
with tempfile.TemporaryDirectory(prefix='ref-copy-negative-',dir='.lake') as tmp:
    for name,mutation in mutations.items():
        file=pathlib.Path(tmp)/f'{name}.lean'
        file.write_text(head+'import Std.Tactic\n'+mutation+'open Lean in'+body)
        result=subprocess.run(['lake','env','lean',str(file)],capture_output=True,text=True)
        output=result.stdout+result.stderr
        assert result.returncode!=0 and 'depends on forbidden axiom' in output, output
        print(f'{name} dependency rejected by fragment audit')
PYTEST
echo '[verify-ref-copy-fragment] four explicit model theorems, axiom audit, independent kernel recheck and three negative tests passed; Rust/LLBC/pass correspondence remains unproved'
