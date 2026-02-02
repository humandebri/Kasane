// どこで: SQLiteマイグレーション / 何を: SQLファイル適用 / なぜ: スキーマ変更を明確化するため

import { readFileSync } from "node:fs";
import path from "node:path";
import type Database from "better-sqlite3";

const MIGRATIONS_DIR = path.join(__dirname, "..", "migrations");

export const MIGRATIONS = [
  "001_init.sql",
  "002_metrics.sql",
  "003_archive.sql",
] as const;

export function applyMigrations(db: Database.Database, migrations: readonly string[]): void {
  db.exec(`
    create table if not exists schema_migrations(
      id text primary key,
      applied_at integer not null
    );
  `);

  const rows = db.prepare<[], { id: string }>("select id from schema_migrations").all();
  const applied = new Set<string>(rows.map((row) => row.id));

  const insert = db.prepare(
    "insert or ignore into schema_migrations(id, applied_at) values(?, ?)"
  );

  if (migrations.length === 0) {
    return;
  }

  db.exec("begin immediate");
  try {
    for (const file of migrations) {
      if (applied.has(file)) {
        continue;
      }
      const sql = readFileSync(path.join(MIGRATIONS_DIR, file), "utf8");
      db.exec(sql);
      insert.run(file, Date.now());
      applied.add(file);
    }
    db.exec("commit");
  } catch (err) {
    db.exec("rollback");
    throw err;
  }
}
