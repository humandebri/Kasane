#!/usr/bin/env bash
set -euo pipefail

UNIT_NAME="${1:-}"
if [[ -z "${UNIT_NAME}" ]]; then
  exit 0
fi

if [[ -f /etc/default/receipt-watch ]]; then
  # shellcheck disable=SC1091
  source /etc/default/receipt-watch
fi

WEBHOOK_URL="${ALERT_WEBHOOK_URL:-}"
if [[ -z "${WEBHOOK_URL}" ]]; then
  exit 0
fi

HOSTNAME_VALUE="$(hostname 2>/dev/null || echo unknown-host)"
TS_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
STATE="$(systemctl show "${UNIT_NAME}" -p ActiveState --value 2>/dev/null || echo unknown)"
SUBSTATE="$(systemctl show "${UNIT_NAME}" -p SubState --value 2>/dev/null || echo unknown)"
RESULT="$(systemctl show "${UNIT_NAME}" -p Result --value 2>/dev/null || echo unknown)"
MAIN_STATUS="$(systemctl show "${UNIT_NAME}" -p ExecMainStatus --value 2>/dev/null || echo unknown)"
N_RESTARTS="$(systemctl show "${UNIT_NAME}" -p NRestarts --value 2>/dev/null || echo unknown)"
LAST_LINE="$(journalctl -u "${UNIT_NAME}" -n 1 --no-pager 2>/dev/null | tail -n 1 || true)"

PAYLOAD="$({
  UNIT_NAME="${UNIT_NAME}" \
  HOSTNAME_VALUE="${HOSTNAME_VALUE}" \
  TS_UTC="${TS_UTC}" \
  STATE="${STATE}" \
  SUBSTATE="${SUBSTATE}" \
  RESULT="${RESULT}" \
  MAIN_STATUS="${MAIN_STATUS}" \
  N_RESTARTS="${N_RESTARTS}" \
  LAST_LINE="${LAST_LINE}" \
  python3 - <<'PY'
import json
import os

unit = os.environ["UNIT_NAME"]
host = os.environ["HOSTNAME_VALUE"]
ts = os.environ["TS_UTC"]
state = os.environ["STATE"]
substate = os.environ["SUBSTATE"]
result = os.environ["RESULT"]
main_status = os.environ["MAIN_STATUS"]
restarts = os.environ["N_RESTARTS"]
last_line = os.environ.get("LAST_LINE", "")

content = (
    f"[kasane-alert] {unit} failed on {host} at {ts} UTC | "
    f"state={state}/{substate} result={result} main_status={main_status} restarts={restarts} | "
    f"last={last_line}"
)

print(json.dumps({
    "content": content,
    "unit": unit,
    "host": host,
    "timestamp_utc": ts,
    "state": state,
    "substate": substate,
    "result": result,
    "exec_main_status": main_status,
    "restarts": restarts,
}, ensure_ascii=False))
PY
})"

curl -sS -m 10 -X POST "${WEBHOOK_URL}" \
  -H "content-type: application/json" \
  --data "${PAYLOAD}" >/dev/null || true
