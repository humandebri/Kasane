#!/usr/bin/env bash
# where: metrics_daily snapshot helper
# what: print latest metrics_daily row and optional day-over-day deltas
# why: 24h実測の数値を即座に確認するため
set -euo pipefail

DB_PATH="${DB_PATH:-${1:-}}"
if [[ -z "${DB_PATH}" ]]; then
  echo "usage: DB_PATH=/path/to/indexer.sqlite scripts/indexer_metrics_snapshot.sh" >&2
  exit 1
fi

python - <<PY
import sqlite3, sys
path = "${DB_PATH}"
conn = sqlite3.connect(path)
rows = conn.execute(
  "select day, raw_bytes, compressed_bytes, sqlite_bytes, archive_bytes, blocks_ingested "
  "from metrics_daily order by day desc limit 2"
).fetchall()
conn.close()
if not rows:
  print("metrics_daily empty")
  sys.exit(0)
latest = rows[0]
print(
  "latest day={0} raw_bytes={1} compressed_bytes={2} sqlite_bytes={3} archive_bytes={4} blocks_ingested={5}".format(
    *latest
  )
)
if len(rows) > 1:
  prev = rows[1]
  deltas = [
    latest[1] - prev[1],
    latest[2] - prev[2],
    (latest[3] - prev[3]) if latest[3] is not None and prev[3] is not None else None,
    (latest[4] - prev[4]) if latest[4] is not None and prev[4] is not None else None,
    latest[5] - prev[5],
  ]
  print(
    "delta raw_bytes={0} compressed_bytes={1} sqlite_bytes={2} archive_bytes={3} blocks_ingested={4}".format(
      *deltas
    )
  )
PY
