#!/usr/bin/env bash
# where: Verus verification entrypoint
# what: verify crates/verified-* source contracts
# why: CIで証明対象ロジックの退行を止めるため
set -euo pipefail

VERUS_BIN="${VERUS_BIN:-verus}"

if ! command -v "${VERUS_BIN}" >/dev/null 2>&1; then
  echo "[verify-verus] missing Verus binary: ${VERUS_BIN}" >&2
  echo "[verify-verus] install Verus or set VERUS_BIN=/path/to/verus" >&2
  exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
work_dir="$(mktemp -d "${TMPDIR:-/tmp}/kasane-verus.XXXXXX")"
trap 'rm -rf "${work_dir}"' EXIT

verified_roots=()
while IFS= read -r lib_rs; do
  verified_roots+=("${lib_rs}")
done < <(find "${repo_root}/crates" -maxdepth 3 -path '*/verified-*/src/lib.rs' | sort)

if [[ "${#verified_roots[@]}" -eq 0 ]]; then
  echo "[verify-verus] no crates/verified-*/src/lib.rs targets found" >&2
  exit 1
fi

for lib_rs in "${verified_roots[@]}"; do
  echo "[verify-verus] ${lib_rs#${repo_root}/}"
  (
    cd "${work_dir}"
    "${VERUS_BIN}" \
      --no-cheating \
      --cfg verus_keep_ghost \
      --edition=2021 \
      --crate-type=lib \
      "${lib_rs}"
  )
done
