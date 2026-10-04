#!/usr/bin/env bash
# Tests an unapplied candidate's scope guards; this is not a semantics proof.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.."
repo_root="$PWD"
shasum -a 256 -c proofs/extraction/ref-copy-candidate-sources.sha256
shasum -a 256 -c proofs/extraction/array-default-sources.sha256
export PATH="${repo_root}/.local/proof-tools/ocaml-bin:${repo_root}/.local/proof-tools/native/bin:$PATH"
export OPAMROOT="${repo_root}/.local/proof-tools/opam-root"
export LIBRARY_PATH="${repo_root}/.local/proof-tools/native/lib"
export CPATH="${repo_root}/.local/proof-tools/native/include"
export DYLD_LIBRARY_PATH="${repo_root}/.local/proof-tools/aeneas/libs${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
eval "$(opam env --switch=5.3.0 --set-root)"
source_root="${repo_root}/.local/proof-tools/aeneas-source"
lock_dir="${repo_root}/.local/proof-tools/ref-copy-candidate.lock"
mkdir "$lock_dir"
backup_file="$(mktemp "${TMPDIR:-/tmp}/kasane-ref-copy-source.XXXXXX")"
cp "$source_root/src/PrePasses.ml" "$backup_file"
restore_source() {
  cp "$backup_file" "$source_root/src/PrePasses.ml"
  rm -f "$backup_file"
  rmdir "$lock_dir"
}
trap restore_source EXIT
patch -p1 -d "$source_root" < proofs/extraction/tool-patches/aeneas-immediate-ref-copy-fusion.patch
mkdir -p "$source_root/src/ref-copy-regression"
cp proofs/extraction/tool-patches/tests/RefCopyRegression.ml "$source_root/src/ref-copy-regression/refCopyRegression.ml"
cat > "$source_root/src/ref-copy-regression/dune" <<'DUNE'
(include_subdirs no)
(executable (name refCopyRegression) (libraries aeneas))
DUNE
(cd "$source_root/src" && dune build ref-copy-regression/refCopyRegression.exe)
"$source_root/src/_build/default/ref-copy-regression/refCopyRegression.exe"   proofs/extraction/tool-patches/fixtures/revm-promoted-mut.json
restore_source
trap - EXIT
shasum -a 256 -c proofs/extraction/array-default-sources.sha256
python3 - <<'PYCHECK'
import hashlib,json,pathlib
profile=json.load(open('proofs/extraction/tool-patches/build-profile.json'))
for filename, expected in profile['experimental_binary_sha256'].items():
    assert hashlib.sha256(pathlib.Path(filename).read_bytes()).hexdigest()==expected, filename
PYCHECK
echo '[verify-ref-copy-candidate] six actual ADD/SUB copy patterns and twelve strict-rejection cases passed; fixed tool source/binaries restored; semantic preservation and EVM instruction correspondence remain unproved'
