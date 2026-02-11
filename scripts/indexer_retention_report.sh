#!/usr/bin/env bash
# where: retention report helper
# what: show latest retention run and current deletion candidates
# why: 保持運用の状態を即座に確認するため
set -euo pipefail

DATABASE_URL="${INDEXER_DATABASE_URL:-${DATABASE_URL:-${1:-}}}"
RETENTION_DAYS="${INDEXER_RETENTION_DAYS:-90}"

if [[ -z "${DATABASE_URL}" ]]; then
  echo "usage: INDEXER_DATABASE_URL=postgres://... scripts/indexer_retention_report.sh" >&2
  exit 1
fi

INDEXER_DATABASE_URL="${DATABASE_URL}" INDEXER_RETENTION_DAYS="${RETENTION_DAYS}" node - <<'NODE'
const { Client } = require('./tools/indexer/node_modules/pg');

(async () => {
  const client = new Client({ connectionString: process.env.INDEXER_DATABASE_URL });
  await client.connect();
  try {
    const retentionDays = Number(process.env.INDEXER_RETENTION_DAYS || '90');
    const latest = await client.query('select * from retention_runs order by finished_at desc limit 1');
    console.log('latest_retention_run=', JSON.stringify(latest.rows[0] ?? null));

    const candidates = await client.query(
      `with cutoff as (
         select
           floor(extract(epoch from (now() - make_interval(days => $1))))::bigint as cutoff_ts,
           to_char((now() - make_interval(days => $1))::date, 'YYYYMMDD')::integer as cutoff_day
       )
       select
         (select count(*) from txs t join blocks b on b.number = t.block_number, cutoff c where b.timestamp < c.cutoff_ts) as txs,
         (select count(*) from blocks b, cutoff c where b.timestamp < c.cutoff_ts) as blocks,
         (select count(*) from metrics_daily m, cutoff c where m.day < c.cutoff_day) as metrics_daily,
         (select count(*) from archive_parts a, cutoff c where a.created_at < c.cutoff_ts * 1000) as archive_parts`,
      [retentionDays]
    );
    console.log('current_candidates=', JSON.stringify(candidates.rows[0] ?? null));
  } finally {
    await client.end();
  }
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
NODE
