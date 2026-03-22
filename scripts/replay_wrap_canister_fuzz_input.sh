#!/usr/bin/env bash
# where: repo root
# what: replay one saved wrap-canister fuzz input through canfuzz test_one_input mode
# why: make crash or interesting input reproduction a single command

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <input-file>" >&2
  exit 1
fi

INPUT_FILE="$1"
if [[ ! -f "$INPUT_FILE" ]]; then
  echo "input file not found: $INPUT_FILE" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
WRAP_CANISTER_FUZZ_ONE_INPUT="$INPUT_FILE" "$ROOT_DIR/scripts/run_wrap_canister_fuzz.sh"
