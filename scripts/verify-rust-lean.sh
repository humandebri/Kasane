#!/usr/bin/env bash
# Regenerate Lean from the production Rust modules; reject drift and custom axioms.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
cd "${repo_root}"
shasum -a 256 -c proofs/extraction/sources.sha256
proof_tools="${repo_root}/.local/proof-tools/aeneas"
[[ -x "${proof_tools}/charon" && -x "${proof_tools}/aeneas" ]] || {
  echo 'Run bash scripts/install-aeneas.sh first' >&2; exit 1;
}
expected_release="$(python3 -c 'import json; print(json.load(open("proofs/extraction/toolchain.json"))["aeneas"])')"
[[ "$("${proof_tools}/aeneas" -version)" == "aeneas ${expected_release}" ]]
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
if [[ "$(uname -s)" == Darwin ]]; then
  export DYLD_LIBRARY_PATH="${proof_tools}/libs${DYLD_LIBRARY_PATH:+:${DYLD_LIBRARY_PATH}}"
else
  export LD_LIBRARY_PATH="${proof_tools}/libs${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
fi
extract_tmp="$(mktemp -d "${TMPDIR:-/tmp}/kasane-extract.XXXXXX")"
trap 'rm -rf "${extract_tmp}"' EXIT
starts=()
for item in account_is_empty account_commit_decision storage_commit_decision code_commit_decision; do
  starts+=(--start-from "kasane_extracted::state_diff::${item}")
done
for item in effective_gas_price l2_fee total_fee base_fee_reward min_fee_satisfied; do
  starts+=(--start-from "kasane_extracted::fee::${item}")
done
"${proof_tools}/charon" cargo --preset=aeneas --sysroot default \
  "${starts[@]}" --start-from 'kasane_extracted::unwrap_dispatch' \
  --dest-file "${extract_tmp}/kasane_extracted.llbc" \
  -- --manifest-path proofs/extraction/rust/Cargo.toml
"${proof_tools}/aeneas" -backend lean -dest "${extract_tmp}" -namespace Extracted \
  -use-lean-modules false -abort-on-error -warnings-as-errors "${extract_tmp}/kasane_extracted.llbc"
diff -u proofs/extraction/lean/KasaneExtracted.lean "${extract_tmp}/KasaneExtracted.lean"
# The word helpers must use the same ruint version and features as production.
production_word_profile="$(cargo tree -p ruint --locked --depth 0 --format '{p} {f}')"
[[ "${production_word_profile}" == 'ruint v1.20.0 alloc,alloy-rlp,serde,std' ]]
cargo metadata --manifest-path proofs/extraction/word-rust/Cargo.toml --locked \
  --format-version 1 > "${extract_tmp}/word-metadata.json"
python3 - "${extract_tmp}/word-metadata.json" <<'WORD_PROFILE'
import hashlib, json, pathlib, sys
metadata = json.load(open(sys.argv[1]))
package = next(p for p in metadata["packages"] if p["name"] == "ruint")
assert package["version"] == "1.20.0", "ruint version drift"
node = next(n for n in metadata["resolve"]["nodes"] if n["id"] == package["id"])
assert set(node["features"]) == {"alloc", "alloy-rlp", "serde", "std"}, "ruint feature drift"
source = pathlib.Path(package["manifest_path"]).parent / "src/algorithms/add.rs"
assert hashlib.sha256(source.read_bytes()).hexdigest() == \
    "af2faf9eb57105865d319ef483b64a065c92a92baf9ee76ef4b0b9cc2b48819d", "ruint source drift"
WORD_PROFILE
"${proof_tools}/charon" cargo --preset=aeneas --sysroot default \
  --start-from 'kasane_word_extracted::carrying_add' \
  --start-from 'kasane_word_extracted::borrowing_sub' \
  --include 'ruint::algorithms::add::carrying_add' \
  --include 'ruint::algorithms::add::borrowing_sub' \
  --dest-file "${extract_tmp}/kasane_word_extracted.llbc" \
  -- --manifest-path proofs/extraction/word-rust/Cargo.toml --locked
"${proof_tools}/aeneas" -backend lean -dest "${extract_tmp}" -namespace WordExtracted \
  -use-lean-modules false -abort-on-error -warnings-as-errors "${extract_tmp}/kasane_word_extracted.llbc"
diff -u proofs/extraction/lean/KasaneWordExtracted.lean "${extract_tmp}/KasaneWordExtracted.lean"
cd proofs/extraction/lean
lake build
lake env lean -DwarningAsError=true Audit.lean
lake env lean -DwarningAsError=true Correspondence.lean
lake env lean -DwarningAsError=true WordCorrespondence.lean
lake env lean -DwarningAsError=true LimbComposition.lean
lake env leanchecker LimbComposition
lake env leanchecker Correspondence
lake env leanchecker WordCorrespondence
echo '[verify-rust-lean] regeneration, all-input proofs, axiom audit and kernel recheck passed'
