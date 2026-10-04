#!/usr/bin/env bash
# Isolated fixed-target array-reference/NonNull representation classifier; memory correspondence remains unproved.
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
for f,h in json.load(open('proofs/extraction/tool-patches/array-nonnull-pure-profile.json'))['base_source_sha256'].items():
    assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
for f,h in json.load(open('proofs/extraction/tool-patches/build-profile.json'))['experimental_binary_sha256'].items():
    assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
PYBASE
lock="${repo_root}/.local/proof-tools/array-nonnull-pure.lock"
mkdir "$lock"
backup_dir="$(mktemp -d "${TMPDIR:-/tmp}/kasane-range-pure.XXXXXX")"
files=(interp/InterpBorrowsCore.ml pure/ReorderDecls.ml interp/InterpExpressions.mli PrePasses.ml llbc/Contexts.ml charon/charon-ml/src/Substitute.ml interp/InterpUtils.ml llbc/TypesUtils.ml llbc/TypesAnalysis.ml pure/Pure.ml pure/PureTypeCheck.ml pure/PrintPure.ml extract/ExtractBase.ml extract/ExtractTypes.ml symbolic/SymbolicToPureTypes.ml interp/InterpExpressions.ml symbolic/SymbolicToPureExpressions.ml extract/Extract.ml pure/PureMicroPassesGeneral.ml pure/PureMicroPassesAnnots.ml extract/ExtractBuiltin.ml interp/InterpStatements.ml pure/PureMicroPassesLoops.ml pure/PureUtils.ml symbolic/SymbolicToPure.ml)
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
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-formatting-source-builtins.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-closed-fnptr-signature.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-fn-region-scope.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-fn-region-diagnostic.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/charon-erasure-local-binders.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-fn-body-erasure.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/charon-trait-fndef-signature.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-fn-item-reification.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-fn-region-inventory.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-fn-value-erasure.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-fn-erased-type-scope.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-fnptr-stored-borrows.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-fnptr-transmute-representation.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-fn-region-predicate.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-trait-fn-item-template.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-fn-item-pure-coercion.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-fnptr-type-template.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-fnptr-pure-transmute.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-trait-fn-item-value.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-notnull-pointer-pattern.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-array-nonnull-representation.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-array-nonnull-symbolic-regions.patch
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-array-nonnull-pure-origin.patch
mkdir -p "$source_root/src/array-nonnull-pure"
cp "$source_root/src/Main.ml" "$source_root/src/array-nonnull-pure/arrayNonNullPureCandidate.ml"
cp proofs/extraction/tool-patches/tests/FormattingSourceOptions.ml "$source_root/src/array-nonnull-pure/formattingSourceOptions.ml"
cp proofs/extraction/tool-patches/tests/NonNullSourceOptions.ml "$source_root/src/array-nonnull-pure/nonNullSourceOptions.ml"
python3 - "$source_root/src/array-nonnull-pure/arrayNonNullPureCandidate.ml" <<'PYMAIN'
import pathlib,sys
p=pathlib.Path(sys.argv[1]);s=p.read_text()
old='(m.options.preset = Some Aeneas)'
assert s.count(old)==1
s=s.replace(old,'(m.options.preset = Some Aeneas || FormattingSourceOptions.formatting_source_options m.options || NonNullSourceOptions.nonnull_source_options m.options)')
p.write_text(s)
PYMAIN
cp proofs/extraction/tool-patches/tests/ArrayNonNullPureRegression.ml "$source_root/src/array-nonnull-pure/arrayNonNullPureRegression.ml"
cat > "$source_root/src/array-nonnull-pure/dune" <<'DUNE'
(include_subdirs no)
(executables
 (names arrayNonNullPureCandidate arrayNonNullPureRegression)
 (libraries aeneas)
 (preprocess (pps ppx_deriving.show ppx_deriving.eq ppx_deriving.ord visitors.ppx aeneas.ppx)))
DUNE
(cd "$source_root/src" && dune build array-nonnull-pure/arrayNonNullPureCandidate.exe array-nonnull-pure/arrayNonNullPureRegression.exe)
(cd "$source_root/src/charon" && dune build charon-ml/tests/Tests.exe && dune runtest --force charon-ml/tests)
mkdir -p .local/proof-tools/aeneas-array-nonnull-pure
for candidate in aeneas check; do
    if [[ -f .local/proof-tools/aeneas-array-nonnull-pure/$candidate ]]; then
        chmod u+w .local/proof-tools/aeneas-array-nonnull-pure/$candidate
    fi
done
cp "$source_root/src/_build/default/array-nonnull-pure/arrayNonNullPureCandidate.exe" .local/proof-tools/aeneas-array-nonnull-pure/aeneas
cp "$source_root/src/_build/default/array-nonnull-pure/arrayNonNullPureRegression.exe" .local/proof-tools/aeneas-array-nonnull-pure/check
restore
trap - EXIT
