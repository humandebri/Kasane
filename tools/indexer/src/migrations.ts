// どこで: Postgresマイグレーション / 何を: SQLファイル適用 / なぜ: スキーマ変更を明確化するため

import { readFileSync } from "node:fs";
import path from "node:path";
import type { Pool } from "pg";

const MIGRATIONS_DIR = path.join(__dirname, "..", "migrations");

export const MIGRATIONS = [
  "001_init.sql",
  "002_backfill.sql",
  "003_add_txs_caller_principal_index.sql",
  "004_add_txs_from_to_addresses.sql",
  "005_add_receipt_status_and_ops_metrics.sql",
  "006_add_token_transfers.sql",
  "007_add_ops_metrics_cycles.sql",
] as const;

export async function applyMigrations(pool: Pool, migrations: readonly string[]): Promise<void> {
  if (migrations.length === 0) {
    return;
  }
  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    await client.query(`
      create table if not exists schema_migrations(
        id text primary key,
        applied_at bigint not null
      );
    `);
    const rows = await client.query<{ id: string }>("select id from schema_migrations");
    const applied = new Set<string>(rows.rows.map((row) => row.id));
    for (const file of migrations) {
      if (applied.has(file)) {
        continue;
      }
      const sql = readFileSync(path.join(MIGRATIONS_DIR, file), "utf8");
      await client.query(sql);
      await client.query("insert into schema_migrations(id, applied_at) values($1, $2)", [file, Date.now()]);
    }
    await client.query("COMMIT");
  } catch (err) {
    await client.query("ROLLBACK");
    throw err;
  } finally {
    client.release();
  }
}
