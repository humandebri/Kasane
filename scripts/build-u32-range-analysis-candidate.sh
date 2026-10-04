#!/usr/bin/env bash
# Isolated, unadopted type-analysis candidate; no range erasure or transmute support.
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
for f,h in json.load(open('proofs/extraction/tool-patches/build-profile.json'))['experimental_binary_sha256'].items():
    assert hashlib.sha256(pathlib.Path(f).read_bytes()).hexdigest()==h,f
PYBASE
lock="${repo_root}/.local/proof-tools/u32-range-analysis.lock"
mkdir "$lock"
backup="$(mktemp "${TMPDIR:-/tmp}/kasane-range-analysis.XXXXXX")"
cp "$source_root/src/llbc/TypesAnalysis.ml" "$backup"
restore() { cp "$backup" "$source_root/src/llbc/TypesAnalysis.ml"; rm -f "$backup"; rmdir "$lock"; }
trap restore EXIT
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-u32-range-borrow-analysis.patch
mkdir -p "$source_root/src/u32-range-analysis"
cp "$source_root/src/Main.ml" "$source_root/src/u32-range-analysis/rangeAnalysisCandidate.ml"
cp proofs/extraction/tool-patches/tests/U32PatternAnalysisRegression.ml "$source_root/src/u32-range-analysis/rangeAnalysisRegression.ml"
cat > "$source_root/src/u32-range-analysis/dune" <<'DUNE'
(include_subdirs no)
(executables
 (names rangeAnalysisCandidate rangeAnalysisRegression)
 (libraries aeneas)
 (preprocess (pps ppx_deriving.show ppx_deriving.eq ppx_deriving.ord visitors.ppx aeneas.ppx)))
DUNE
(cd "$source_root/src" && dune build u32-range-analysis/rangeAnalysisCandidate.exe u32-range-analysis/rangeAnalysisRegression.exe)
mkdir -p .local/proof-tools/aeneas-u32-range-analysis
for candidate in aeneas check; do
    if [[ -f .local/proof-tools/aeneas-u32-range-analysis/$candidate ]]; then
        chmod u+w .local/proof-tools/aeneas-u32-range-analysis/$candidate
    fi
done
cp "$source_root/src/_build/default/u32-range-analysis/rangeAnalysisCandidate.exe" .local/proof-tools/aeneas-u32-range-analysis/aeneas
cp "$source_root/src/_build/default/u32-range-analysis/rangeAnalysisRegression.exe" .local/proof-tools/aeneas-u32-range-analysis/check
restore
trap - EXIT
