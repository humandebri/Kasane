#!/usr/bin/env bash
# Verify the executable Verus fee contracts and the pinned Lean/revm correspondence.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bash "${script_dir}/verify-verus.sh"
bash "${script_dir}/verify-revm.sh"
