#!/usr/bin/env bash
# どこで: canister wasm サイズ診断
# 何を: wasm の依存寄与を可視化し、削減候補の当たり筋を固定化する
# なぜ: 勘ではなく、支配的シンボルと依存木に基づいて工数を最小化するため
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"
TARGET="wasm32-unknown-unknown"
PACKAGE="ic-evm-gateway"
OUT_DIR=""
INPUT_WASM=""
COMPARE_DIR=""
SKIP_BUILD=0
TOP_N="${TOP_N:-120}"
DOMINATORS_ROWS="${DOMINATORS_ROWS:-160}"
usage() {
  cat <<'EOF'
Usage:
  scripts/profile_wasm_deps.sh [options]

Options:
  --package <name>        Cargo package name (default: ic-evm-gateway)
  --target <triple>       Cargo target triple (default: wasm32-unknown-unknown)
  --wasm <path>           Use an existing wasm file and skip automatic path resolution
  --skip-build            Skip cargo build step (requires --wasm or prebuilt target artifact)
  --out <dir>             Output directory (default: docs/ops/reports/wasm-deps-<pkg>-<timestamp>)
  --compare <dir>         Compare with previous output directory (expects metrics.env)
  --top-n <N>             Max rows for twiggy top (default: 120)
  --dom-rows <N>          Max rows for twiggy dominators (default: 160)
  -h, --help              Show this help

Examples:
  scripts/profile_wasm_deps.sh
  scripts/profile_wasm_deps.sh --package ic-evm-gateway --compare docs/ops/reports/wasm-deps-ic-evm-gateway-20260305-101500
  scripts/profile_wasm_deps.sh --wasm /tmp/ic_evm_gateway.wasm --skip-build
EOF
}
log() {
  echo "[profile-wasm-deps] $*"
}
need_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[profile-wasm-deps] missing command: $cmd" >&2
    exit 1
  fi
}
sanitize_digits() {
  tr -cd '0-9'
}
while [[ $# -gt 0 ]]; do
  case "$1" in
    --package)
      PACKAGE="$2"
      shift 2
      ;;
    --target)
      TARGET="$2"
      shift 2
      ;;
    --wasm)
      INPUT_WASM="$2"
      shift 2
      ;;
    --skip-build)
      SKIP_BUILD=1
      shift
      ;;
    --out)
      OUT_DIR="$2"
      shift 2
      ;;
    --compare)
      COMPARE_DIR="$2"
      shift 2
      ;;
    --top-n)
      TOP_N="$2"
      shift 2
      ;;
    --dom-rows)
      DOMINATORS_ROWS="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "[profile-wasm-deps] unknown arg: $1" >&2
      usage
      exit 2
      ;;
  esac
done
need_cmd cargo
need_cmd twiggy
need_cmd wasm-tools
need_cmd rg
need_cmd awk
STAMP="$(date +%Y%m%d-%H%M%S)"
STAMP_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
if [[ -z "$OUT_DIR" ]]; then
  OUT_DIR="docs/ops/reports/wasm-deps-${PACKAGE}-${STAMP}"
fi
mkdir -p "$OUT_DIR"
WASM_PATH=""
if [[ -n "$INPUT_WASM" ]]; then
  WASM_PATH="$INPUT_WASM"
else
  WASM_PATH="target/${TARGET}/release/${PACKAGE//-/_}.wasm"
fi
if [[ "$SKIP_BUILD" -eq 0 && -z "$INPUT_WASM" ]]; then
  log "building package=${PACKAGE} target=${TARGET} (release)"
  cargo build --release --target "$TARGET" -p "$PACKAGE"
fi
if [[ ! -f "$WASM_PATH" ]]; then
  echo "[profile-wasm-deps] wasm not found: $WASM_PATH" >&2
  exit 2
fi
COPIED_WASM="${OUT_DIR}/${PACKAGE//-/_}.wasm"
DEMANGLED_WASM="${OUT_DIR}/${PACKAGE//-/_}.demangled.wasm"
TWIGGY_TOP_FILE="${OUT_DIR}/twiggy.top.txt"
TWIGGY_TOP_RETAINED_FILE="${OUT_DIR}/twiggy.top.retained.txt"
TWIGGY_DOM_FILE="${OUT_DIR}/twiggy.dominators.txt"
OBJDUMP_FILE="${OUT_DIR}/wasm.objdump.txt"
WAT_FILE="${OUT_DIR}/wasm.print.wat"
METRICS_FILE="${OUT_DIR}/metrics.env"
SUMMARY_FILE="${OUT_DIR}/summary.md"
COMPARE_FILE="${OUT_DIR}/compare.md"
cp "$WASM_PATH" "$COPIED_WASM"
log "copied wasm => $COPIED_WASM"
wasm-tools demangle "$COPIED_WASM" -o "$DEMANGLED_WASM"
log "demangled wasm => $DEMANGLED_WASM"
twiggy top -n "$TOP_N" "$DEMANGLED_WASM" >"$TWIGGY_TOP_FILE"
twiggy top --retained -n "$TOP_N" "$DEMANGLED_WASM" >"$TWIGGY_TOP_RETAINED_FILE"
twiggy dominators -r "$DOMINATORS_ROWS" "$DEMANGLED_WASM" >"$TWIGGY_DOM_FILE"
log "twiggy reports generated"
wasm-tools objdump "$COPIED_WASM" >"$OBJDUMP_FILE"
if wasm-tools print "$DEMANGLED_WASM" >"$WAT_FILE" 2>"${OUT_DIR}/wasm.print.stderr.log"; then
  :
else
  echo "[profile-wasm-deps] WARN: wasm-tools print failed, instruction estimate becomes 0" >&2
  : >"$WAT_FILE"
fi

# Optional cargo bloat with nightly -Z build-std
if cargo +nightly -V >/dev/null 2>&1; then
  if cargo +nightly bloat -Z build-std=std,panic_abort --release --target "$TARGET" -p "$PACKAGE" --crates -n "$TOP_N" >"${OUT_DIR}/cargo-bloat.nightly.txt" 2>&1; then
    log "cargo +nightly bloat (-Z build-std) completed"
  else
    echo "[profile-wasm-deps] WARN: nightly bloat failed. see ${OUT_DIR}/cargo-bloat.nightly.txt" >&2
  fi
else
  echo "[profile-wasm-deps] INFO: nightly toolchain not available. skip cargo bloat -Z build-std." >"${OUT_DIR}/cargo-bloat.nightly.txt"
fi
declare -a suspects=(
  "candid_stack:candid"
  "alloy_stack:alloy-consensus"
  "precompile_math:revm-precompile"
  "kzg_stack:c-kzg"
  "ic_stable_structures:ic-stable-structures"
)
for entry in "${suspects[@]}"; do
  label="${entry%%:*}"
  crate="${entry##*:}"
  output="${OUT_DIR}/cargo-tree.${label}.txt"
  if cargo tree --workspace --target "$TARGET" -e features -i "$crate" >"$output" 2>&1; then
    :
  else
    echo "[profile-wasm-deps] INFO: crate not found in dependency graph for target=$TARGET: $crate" >"$output"
  fi
done
COMBINED_FILE="${OUT_DIR}/twiggy.combined.txt"
cat "$TWIGGY_TOP_RETAINED_FILE" "$TWIGGY_DOM_FILE" >"$COMBINED_FILE"
extract_culprit() {
  local key="$1"
  local pattern="$2"
  local tmp_file="${OUT_DIR}/culprit.${key}.tmp.txt"
  local out_file="${OUT_DIR}/culprit.${key}.txt"
  rg -n -i "$pattern" "$COMBINED_FILE" >"$tmp_file" || true
  awk 'NR<=80 { print }' "$tmp_file" >"$out_file"
  rm -f "$tmp_file"
  if [[ ! -s "$out_file" ]]; then
    echo "(no match)" >"$out_file"
  fi
}
extract_culprit "candid_stack" "candid|did"
extract_culprit "alloy_stack" "alloy|eip2930|eip1559|eip4844|eip7702|k256"
extract_culprit "precompile_math" "precompile|bls|kzg|blob|4844|c-kzg|bn254|modexp|ark_|secp256r1|p256"
extract_culprit "kzg_stack" "kzg|blob|4844|c-kzg"
extract_culprit "ic_stable_structures" "ic_stable_structures|stable_structures|stable_btree"
WASM_BYTES="$(wc -c <"$COPIED_WASM" | sanitize_digits)"
CODE_SECTION_BYTES="$(awk -F'|' '/^[[:space:]]*code[[:space:]]*\|/ {print $3; exit}' "$OBJDUMP_FILE" | sanitize_digits)"
FUNCTION_COUNT="$(awk -F'|' '/^[[:space:]]*functions[[:space:]]*\|/ {print $4; exit}' "$OBJDUMP_FILE" | sanitize_digits)"
if [[ -z "$FUNCTION_COUNT" ]]; then
  FUNCTION_COUNT="0"
fi
if [[ -z "$CODE_SECTION_BYTES" ]]; then
  CODE_SECTION_BYTES="0"
fi
INSTRUCTION_ESTIMATE="$(
  rg -c "^[[:space:]]+\\((unreachable|nop|block|loop|if|br|br_if|br_table|return|call|call_indirect|drop|select|local\\.|global\\.|table\\.|memory\\.|i32\\.|i64\\.|f32\\.|f64\\.|v128\\.|ref\\.)" "$WAT_FILE" 2>/dev/null || echo 0
)"
if [[ -z "$INSTRUCTION_ESTIMATE" ]]; then
  INSTRUCTION_ESTIMATE="0"
fi
cat >"$METRICS_FILE" <<EOF
GENERATED_AT_UTC=${STAMP_UTC}
PACKAGE=${PACKAGE}
TARGET=${TARGET}
WASM_SOURCE=${WASM_PATH}
WASM_BYTES=${WASM_BYTES}
CODE_SECTION_BYTES=${CODE_SECTION_BYTES}
FUNCTION_COUNT=${FUNCTION_COUNT}
INSTRUCTION_ESTIMATE=${INSTRUCTION_ESTIMATE}
EOF
if [[ -n "$COMPARE_DIR" && -f "${COMPARE_DIR}/metrics.env" ]]; then
  get_metric() {
    local key="$1"
    local file="$2"
    grep -E "^${key}=" "$file" | head -n1 | cut -d'=' -f2-
  }

  pct_delta() {
    local before="$1"
    local after="$2"
    awk -v b="$before" -v a="$after" 'BEGIN { if (b == 0) { print "n/a"; } else { printf "%.2f%%", ((a - b) / b) * 100; } }'
  }
  BASE_METRICS="${COMPARE_DIR}/metrics.env"
  BASE_WASM_BYTES="$(get_metric WASM_BYTES "$BASE_METRICS")"
  BASE_CODE_BYTES="$(get_metric CODE_SECTION_BYTES "$BASE_METRICS")"
  BASE_FUNCTION_COUNT="$(get_metric FUNCTION_COUNT "$BASE_METRICS")"
  BASE_INSTR="$(get_metric INSTRUCTION_ESTIMATE "$BASE_METRICS")"
  DELTA_WASM=$((WASM_BYTES - BASE_WASM_BYTES))
  DELTA_CODE=$((CODE_SECTION_BYTES - BASE_CODE_BYTES))
  DELTA_FUNC=$((FUNCTION_COUNT - BASE_FUNCTION_COUNT))
  DELTA_INSTR=$((INSTRUCTION_ESTIMATE - BASE_INSTR))
  cat >"$COMPARE_FILE" <<EOF
## Before/After (from --compare)

| metric | before | after | delta | delta% |
|---|---:|---:|---:|---:|
| wasm_bytes | ${BASE_WASM_BYTES} | ${WASM_BYTES} | ${DELTA_WASM} | $(pct_delta "$BASE_WASM_BYTES" "$WASM_BYTES") |
| code_section_bytes | ${BASE_CODE_BYTES} | ${CODE_SECTION_BYTES} | ${DELTA_CODE} | $(pct_delta "$BASE_CODE_BYTES" "$CODE_SECTION_BYTES") |
| function_count | ${BASE_FUNCTION_COUNT} | ${FUNCTION_COUNT} | ${DELTA_FUNC} | $(pct_delta "$BASE_FUNCTION_COUNT" "$FUNCTION_COUNT") |
| instruction_estimate | ${BASE_INSTR} | ${INSTRUCTION_ESTIMATE} | ${DELTA_INSTR} | $(pct_delta "$BASE_INSTR" "$INSTRUCTION_ESTIMATE") |
EOF
fi
{
  echo "# Wasm Dependency Profile"
  echo
  echo "- generated_at_utc: \`${STAMP_UTC}\`"
  echo "- package: \`${PACKAGE}\`"
  echo "- target: \`${TARGET}\`"
  echo "- wasm_source: \`${WASM_PATH}\`"
  echo "- wasm_bytes: \`${WASM_BYTES}\`"
  echo "- code_section_bytes: \`${CODE_SECTION_BYTES}\`"
  echo "- function_count: \`${FUNCTION_COUNT}\`"
  echo "- instruction_estimate: \`${INSTRUCTION_ESTIMATE}\`"
  echo
  echo "## Culprit Map"
  echo "- candid_stack: \`culprit.candid_stack.txt\`"
  echo "- alloy_stack: \`culprit.alloy_stack.txt\`"
  echo "- precompile_math: \`culprit.precompile_math.txt\`"
  echo "- kzg_stack: \`culprit.kzg_stack.txt\`"
  echo "- ic_stable_structures: \`culprit.ic_stable_structures.txt\`"
  echo
  echo "## Primary Artifacts"
  echo "- \`twiggy.top.txt\`"
  echo "- \`twiggy.top.retained.txt\`"
  echo "- \`twiggy.dominators.txt\`"
  echo "- \`cargo-bloat.nightly.txt\`"
  echo "- \`cargo-tree.candid_stack.txt\`"
  echo "- \`cargo-tree.alloy_stack.txt\`"
  echo "- \`cargo-tree.precompile_math.txt\`"
  echo "- \`cargo-tree.kzg_stack.txt\`"
  echo "- \`cargo-tree.ic_stable_structures.txt\`"
  if [[ -f "$COMPARE_FILE" ]]; then
    echo
    cat "$COMPARE_FILE"
  fi
} >"$SUMMARY_FILE"
log "done. output dir => $OUT_DIR"
log "summary => $SUMMARY_FILE"
