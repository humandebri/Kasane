#!/usr/bin/env bash
# where: metrics_daily snapshot helper
# what: print latest metrics_daily row and optional day-over-day deltas
# why: 24h実測の数値を即座に確認するため
set -euo pipefail

DATABASE_URL="${INDEXER_DATABASE_URL:-${DATABASE_URL:-${1:-}}}"
if [[ -z "${DATABASE_URL}" ]]; then
  echo "usage: INDEXER_DATABASE_URL=postgres://... scripts/indexer_metrics_snapshot.sh" >&2
  exit 1
fi

node - <<'NODE'
const { Client } = require('./tools/indexer/node_modules/pg');

(async () => {
  const client = new Client({ connectionString: process.env.INDEXER_DATABASE_URL || process.env.DATABASE_URL || process.argv[1] });
  await client.connect();
  try {
    const { rows } = await client.query(
      'select day, raw_bytes, compressed_bytes, archive_bytes, blocks_ingested from metrics_daily order by day desc limit 2'
    );
    if (rows.length === 0) {
      console.log('metrics_daily empty');
      return;
    }
    const latest = rows[0];
    console.log(
      `latest day=${latest.day} raw_bytes=${latest.raw_bytes} compressed_bytes=${latest.compressed_bytes} archive_bytes=${latest.archive_bytes} blocks_ingested=${latest.blocks_ingested}`
    );
    if (rows.length > 1) {
      const prev = rows[1];
      const delta = {
        raw: Number(latest.raw_bytes) - Number(prev.raw_bytes),
        compressed: Number(latest.compressed_bytes) - Number(prev.compressed_bytes),
        archive:
          latest.archive_bytes === null || prev.archive_bytes === null
            ? null
            : Number(latest.archive_bytes) - Number(prev.archive_bytes),
        blocks: Number(latest.blocks_ingested) - Number(prev.blocks_ingested),
      };
      console.log(
        `delta raw_bytes=${delta.raw} compressed_bytes=${delta.compressed} archive_bytes=${delta.archive} blocks_ingested=${delta.blocks}`
      );
    }
  } finally {
    await client.end();
  }
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
NODE
