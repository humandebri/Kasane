#!/usr/bin/env bash
# All-input source predicate correspondence; not whole AST/GAT correctness.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
shasum -a 256 -c proofs/extraction/typeinfo-sources.sha256
python3 - <<'PYCHECK'
import hashlib,json,pathlib
profile=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
for file,expected in profile['experimental_binary_sha256'].items():
    assert hashlib.sha256(pathlib.Path(file).read_bytes()).hexdigest()==expected,file
PYCHECK
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export CARGO_TARGET_DIR="$PWD/.local/proof-tools/charon-self-extraction-target"
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
extract_tmp="$(mktemp -d "${TMPDIR:-/tmp}/kasane-typeinfo.XXXXXX")"
trap 'rm -rf "$extract_tmp"' EXIT
target/debug/charon cargo --preset=aeneas --mir optimized --error-on-warnings \
  --start-from 'charon_lib::ast::type_level::type_info::TypeInfo::is_closed' \
  --include 'charon_lib::ast::type_level::type_info::_::is_closed' \
  --include 'charon_lib::ast::type_level::type_info::_::mentions_var' \
  --include 'charon_lib::ast::type_level::type_info::_::mentions_self_clause' \
  --include 'charon_lib::ast::type_level::type_info::_::uses_size_metadata' \
  --dest-file "$extract_tmp/kasane-charon-is-closed.llbc" -- \
  --manifest-path .local/proof-tools/charon-source/charon/Cargo.toml --lib --no-default-features --locked
.local/proof-tools/aeneas-source/src/_build/default/main.exe -backend lean \
  -dest "$extract_tmp" -namespace CharonTypeInfo -use-lean-modules false \
  -abort-on-error -warnings-as-errors "$extract_tmp/kasane-charon-is-closed.llbc"
diff -u proofs/extraction/operator-lean/TypeInfoGenerated.lean "$extract_tmp/Kasane-charon-is-closed.lean"
cd proofs/extraction/operator-lean
lake build TypeInfoCorrespondence TypeInfoAudit
lake env lean -DwarningAsError=true TypeInfoCorrespondence.lean
lake env lean -DwarningAsError=true TypeInfoAudit.lean
lake env leanchecker TypeInfoCorrespondence
python3 - <<'PYTEST'
import pathlib,subprocess,tempfile
head,body=pathlib.Path('TypeInfoAudit.lean').read_text().split('open Lean in',1)
mutations={
 'axiom':'namespace TypeInfoCorrespondence\naxiom bad : False\ntheorem negative : False := bad\nend TypeInfoCorrespondence\n',
 'sorry':'namespace TypeInfoCorrespondence\ntheorem negative : False := by sorry\nend TypeInfoCorrespondence\n',
 'native':'namespace TypeInfoCorrespondence\ntheorem negative : (1 : Nat) = 1 := by native_decide\nend TypeInfoCorrespondence\n'}
with tempfile.TemporaryDirectory(prefix='typeinfo-negative-',dir='.lake') as tmp:
    for case,mutation in mutations.items():
        file=pathlib.Path(tmp)/f'{case}.lean'
        file.write_text(head+'import Std.Tactic\n'+mutation+'open Lean in'+body)
        result=subprocess.run(['lake','env','lean',str(file)],capture_output=True,text=True)
        output=result.stdout+result.stderr
        assert result.returncode!=0 and 'depends on forbidden axiom' in output,output
        print(f'{case} dependency rejected by TypeInfo audit')
PYTEST
echo '[verify-typeinfo-experimental] regeneration, eight explicit all-input source predicate theorems, 31-declaration audit, kernel recheck and negative tests passed; AST flag computation, feature-profile equivalence and GAT transform correctness unproved'
