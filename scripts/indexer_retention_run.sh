#!/usr/bin/env bash
# where: retention manual runner
# what: execute run_retention_cleanup with retention_days and dry_run
# why: pg_cron未導入環境でも手動/cron実行できるようにするため
set -euo pipefail

DATABASE_URL="${INDEXER_DATABASE_URL:-${DATABASE_URL:-${1:-}}}"
RETENTION_DAYS="${INDEXER_RETENTION_DAYS:-90}"
DRY_RUN="${INDEXER_RETENTION_DRY_RUN:-false}"

if [[ -z "${DATABASE_URL}" ]]; then
  echo "usage: INDEXER_DATABASE_URL=postgres://... scripts/indexer_retention_run.sh" >&2
  exit 1
fi

INDEXER_DATABASE_URL="${DATABASE_URL}" INDEXER_RETENTION_DAYS="${RETENTION_DAYS}" INDEXER_RETENTION_DRY_RUN="${DRY_RUN}" node - <<'NODE'
const { Client } = require('./tools/indexer/node_modules/pg');

(async () => {
  const client = new Client({ connectionString: process.env.INDEXER_DATABASE_URL });
  await client.connect();
  try {
    const retentionDays = Number(process.env.INDEXER_RETENTION_DAYS || '90');
    const dryRun = String(process.env.INDEXER_RETENTION_DRY_RUN || 'false').toLowerCase() === 'true' || process.env.INDEXER_RETENTION_DRY_RUN === '1';
    const out = await client.query('select * from run_retention_cleanup($1, $2)', [retentionDays, dryRun]);
    console.log(JSON.stringify(out.rows[0] ?? null, null, 2));
  } finally {
    await client.end();
  }
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
NODE
