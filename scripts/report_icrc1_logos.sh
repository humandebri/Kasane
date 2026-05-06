#!/usr/bin/env bash
# where: repo root
# what: ICRC-1 ledger metadata から logo を収集し markdown report を保存する
# why: token list の静的URL依存を減らし、一次情報の保存場所を repo 内に残すため
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

NETWORK="${NETWORK:-ic}"
REPORT_DIR="${REPORT_DIR:-docs/ops/reports}"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
REPORT_FILE="${REPORT_DIR}/icrc1-logo-report-${TIMESTAMP}.md"

DEFAULT_LEDGERS=(
  "ryjl3-tyaaa-aaaaa-aaaba-cai"
  "mxzaz-hqaaa-aaaar-qaada-cai"
  "ss2fx-dyaaa-aaaar-qacoq-cai"
  "xevnm-gaaaa-aaaar-qafnq-cai"
)

usage() {
  cat <<'EOF'
usage:
  scripts/report_icrc1_logos.sh [LEDGER_CANISTER_ID ...]

env:
  NETWORK=ic
  REPORT_DIR=docs/ops/reports

notes:
  - 引数未指定時は ICP / ckBTC / ckETH / ckUSDC を対象にする
  - ICRC-1 標準に合わせて `icrc1_metadata` から `icrc1:logo` を取得する
EOF
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing command: $1" >&2
    exit 1
  fi
}

collect_metadata_json() {
  local ledger_id="$1"
  dfx canister call --network "${NETWORK}" --identity anonymous --query "${ledger_id}" icrc1_metadata '()' --output json
}

mkdir -p "${REPORT_DIR}"
require_cmd dfx
require_cmd python3

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

LEDGERS=("$@")
if [[ "${#LEDGERS[@]}" -eq 0 ]]; then
  LEDGERS=("${DEFAULT_LEDGERS[@]}")
fi

{
  echo "# ICRC-1 Logo Report"
  echo
  echo "- generated_at_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "- network: ${NETWORK}"
  echo "- source: \`dfx canister call --identity anonymous --query <ledger> icrc1_metadata '()' --output json\`"
  echo
  echo "| ledger | symbol | name | logo | note |"
  echo "| --- | --- | --- | --- | --- |"
} > "${REPORT_FILE}"

for ledger_id in "${LEDGERS[@]}"; do
  if metadata_json="$(collect_metadata_json "${ledger_id}" 2>&1)"; then
    python3 - "${ledger_id}" "${metadata_json}" >> "${REPORT_FILE}" <<'PY'
import json
import sys

ledger_id = sys.argv[1]
payload = json.loads(sys.argv[2])

entries = {}
if not isinstance(payload, list):
    print(f"| `{ledger_id}` | - | - | - | invalid_metadata_shape |")
    raise SystemExit(0)

for item in payload:
    if not isinstance(item, dict):
        continue
    key = item.get("0")
    value = item.get("1")
    if not isinstance(key, str) or not isinstance(value, dict):
        continue
    entries[key] = value

def read_text(key: str):
    value = entries.get(key)
    if not isinstance(value, dict):
        return None
    text = value.get("Text")
    return text if isinstance(text, str) and text != "" else None

symbol = read_text("icrc1:symbol") or "-"
name = read_text("icrc1:name") or "-"
logo = read_text("icrc1:logo")
note = "ok" if logo else "logo_missing"
logo_cell = f"`{logo}`" if logo else "-"
print(f"| `{ledger_id}` | {symbol} | {name} | {logo_cell} | {note} |")
PY
  else
    printf '| `%s` | - | - | - | query_failed: `%s` |\n' "${ledger_id}" "$(printf '%s' "${metadata_json}" | tr '\n' ' ')" >> "${REPORT_FILE}"
  fi
done

echo "report=${REPORT_FILE}"
