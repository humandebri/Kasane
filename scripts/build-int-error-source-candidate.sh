#!/usr/bin/env bash
# Isolated source TryFromIntError; retain actual payload instead of backend Unit.
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
for f,h in json.load(open('proofs/extraction/tool-patches/int-error-source-profile.json'))['base_source_sha256'].items():
    assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
for f,h in json.load(open('proofs/extraction/tool-patches/build-profile.json'))['experimental_binary_sha256'].items():
    assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
PYBASE
lock="${repo_root}/.local/proof-tools/int-error-source.lock"
mkdir "$lock"
backup_dir="$(mktemp -d "${TMPDIR:-/tmp}/kasane-range-pure.XXXXXX")"
files=(llbc/TypesAnalysis.ml pure/Pure.ml pure/PureTypeCheck.ml pure/PrintPure.ml extract/ExtractBase.ml extract/ExtractTypes.ml symbolic/SymbolicToPureTypes.ml interp/InterpExpressions.ml symbolic/SymbolicToPureExpressions.ml extract/Extract.ml pure/PureMicroPassesGeneral.ml pure/PureMicroPassesAnnots.ml extract/ExtractBuiltin.ml)
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
mkdir -p "$source_root/src/int-error-source"
cp "$source_root/src/Main.ml" "$source_root/src/int-error-source/intErrorCandidate.ml"
cp proofs/extraction/tool-patches/tests/IntErrorSourceRegression.ml "$source_root/src/int-error-source/intErrorRegression.ml"
cat > "$source_root/src/int-error-source/dune" <<'DUNE'
(include_subdirs no)
(executables
 (names intErrorCandidate intErrorRegression)
 (libraries aeneas)
 (preprocess (pps ppx_deriving.show ppx_deriving.eq ppx_deriving.ord visitors.ppx aeneas.ppx)))
DUNE
(cd "$source_root/src" && dune build int-error-source/intErrorCandidate.exe int-error-source/intErrorRegression.exe)
mkdir -p .local/proof-tools/aeneas-int-error-source
for candidate in aeneas check; do
    if [[ -f .local/proof-tools/aeneas-int-error-source/$candidate ]]; then
        chmod u+w .local/proof-tools/aeneas-int-error-source/$candidate
    fi
done
cp "$source_root/src/_build/default/int-error-source/intErrorCandidate.exe" .local/proof-tools/aeneas-int-error-source/aeneas
cp "$source_root/src/_build/default/int-error-source/intErrorRegression.exe" .local/proof-tools/aeneas-int-error-source/check
restore
trap - EXIT
