#!/usr/bin/env bash
# Isolated dynamic Debug and retained-panic profile; keep the fixed tools unchanged.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
repo_root="$PWD"
export PATH="${repo_root}/.local/proof-tools/ocaml-bin:${repo_root}/.local/proof-tools/native/bin:$PATH"
export OPAMROOT="${repo_root}/.local/proof-tools/opam-root"
export LIBRARY_PATH="${repo_root}/.local/proof-tools/native/lib"
export CPATH="${repo_root}/.local/proof-tools/native/include"
export DYLD_LIBRARY_PATH="${repo_root}/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
eval "$(opam env --switch=5.3.0 --set-root)"
source_root="${repo_root}/.local/proof-tools/aeneas-source"
python3 - <<'PYBASE'
import hashlib,json,pathlib
p=pathlib.Path('.local/proof-tools/aeneas-source/src/llbc/TypesAnalysis.ml')
assert hashlib.sha256(p.read_bytes()).hexdigest()=='9d4c6eb7be147df8a9fdc1cf486adadf3fc2da4a478a050ed1f0207f0368fd7b'
for f,h in json.load(open('proofs/extraction/tool-patches/dyn-debug-profile.json'))['base_source_sha256'].items():
    assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
for f,h in json.load(open('proofs/extraction/tool-patches/build-profile.json'))['experimental_binary_sha256'].items():
    assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
PYBASE
lock="${repo_root}/.local/proof-tools/dyn-debug.lock"
mkdir "$lock"
backup_dir="$(mktemp -d "${TMPDIR:-/tmp}/kasane-range-pure.XXXXXX")"
files=(llbc/TypesAnalysis.ml pure/Pure.ml pure/PureTypeCheck.ml pure/PrintPure.ml extract/ExtractBase.ml extract/ExtractTypes.ml symbolic/SymbolicToPureTypes.ml interp/InterpExpressions.ml symbolic/SymbolicToPureExpressions.ml extract/Extract.ml pure/PureMicroPassesGeneral.ml pure/PureMicroPassesAnnots.ml extract/ExtractBuiltin.ml interp/InterpStatements.ml pure/PureMicroPassesLoops.ml pure/PureUtils.ml symbolic/SymbolicToPure.ml)
for file in "${files[@]}"; do
    mkdir -p "$backup_dir/$(dirname "$file")"
    cp "$source_root/src/$file" "$backup_dir/$file"
done
restore() {
    for file in "${files[@]}"; do cp "$backup_dir/$file" "$source_root/src/$file"; done
    rm -rf "$backup_dir"
    rmdir "$lock"
}
trap restore EXIT
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-u32-range-borrow-analysis.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-u32-range-pure-subtype.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-nanoseconds-read-layout.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-nanoseconds-construct-partial.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-int-error-source-type.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-never-call-elimination.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-dyn-debug-instance.patch
mkdir -p "$source_root/src/dyn-debug"
cp "$source_root/src/Main.ml" "$source_root/src/dyn-debug/dynDebugCandidate.ml"
cp proofs/extraction/tool-patches/tests/RetainedPanicOptions.ml "$source_root/src/dyn-debug/retainedPanicOptions.ml"
python3 - "$source_root/src/dyn-debug/dynDebugCandidate.ml" <<'PYMAIN'
import pathlib,sys
p=pathlib.Path(sys.argv[1]);s=p.read_text()
old='(m.options.preset = Some Aeneas)'
assert s.count(old)==1
s=s.replace(old,'(m.options.preset = Some Aeneas || RetainedPanicOptions.retained_panic_options m.options)')
p.write_text(s)
PYMAIN
cp proofs/extraction/tool-patches/tests/DynDebugRegression.ml "$source_root/src/dyn-debug/dynDebugRegression.ml"
cat > "$source_root/src/dyn-debug/dune" <<'DUNE'
(include_subdirs no)
(executables
 (names dynDebugCandidate dynDebugRegression)
 (libraries aeneas)
 (preprocess (pps ppx_deriving.show ppx_deriving.eq ppx_deriving.ord visitors.ppx aeneas.ppx)))
DUNE
(cd "$source_root/src" && dune build dyn-debug/dynDebugCandidate.exe dyn-debug/dynDebugRegression.exe)
mkdir -p .local/proof-tools/aeneas-dyn-debug
for candidate in aeneas check; do
    if [[ -f .local/proof-tools/aeneas-dyn-debug/$candidate ]]; then
        chmod u+w .local/proof-tools/aeneas-dyn-debug/$candidate
    fi
done
cp "$source_root/src/_build/default/dyn-debug/dynDebugCandidate.exe" .local/proof-tools/aeneas-dyn-debug/aeneas
cp "$source_root/src/_build/default/dyn-debug/dynDebugRegression.exe" .local/proof-tools/aeneas-dyn-debug/check
restore
trap - EXIT
