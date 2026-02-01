// どこで: SQLiteアクセス / 何を: スキーマとUPSERT / なぜ: コミット境界を守るため

import Database from "better-sqlite3";
import { cursorFromJson, cursorToJson } from "./cursor";
import { Cursor } from "./types";

export type BlockRow = {
  number: bigint;
  hash: Buffer | null;
  timestamp: bigint;
  tx_count: number;
};

export type TxRow = {
  tx_hash: Buffer;
  block_number: bigint;
  tx_index: number;
};

export class IndexerDb {
  private db: Database.Database;
  private getMetaStmt: Database.Statement;
  private upsertMetaStmt: Database.Statement;
  private upsertBlockStmt: Database.Statement;
  private upsertTxStmt: Database.Statement;
  private upsertMetricsStmt: Database.Statement;

  constructor(path: string) {
    this.db = new Database(path);
    this.db.pragma("journal_mode = WAL");
    this.db.pragma("synchronous = NORMAL");
    this.db.exec(schemaSql());
    this.getMetaStmt = this.db.prepare("SELECT value FROM meta WHERE key = ?");
    this.upsertMetaStmt = this.db.prepare("INSERT INTO meta(key, value) VALUES(?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value");
    this.upsertBlockStmt = this.db.prepare(
      "INSERT INTO blocks(number, hash, timestamp, tx_count) VALUES(?, ?, ?, ?) ON CONFLICT(number) DO UPDATE SET hash = excluded.hash, timestamp = excluded.timestamp, tx_count = excluded.tx_count"
    );
    this.upsertTxStmt = this.db.prepare(
      "INSERT INTO txs(tx_hash, block_number, tx_index) VALUES(?, ?, ?) ON CONFLICT(tx_hash) DO UPDATE SET block_number = excluded.block_number, tx_index = excluded.tx_index"
    );
    this.upsertMetricsStmt = this.db.prepare(
      \"INSERT INTO metrics_daily(day, raw_bytes, compressed_bytes, sqlite_growth_bytes, blocks_ingested, errors) VALUES(?, ?, ?, ?, ?, ?) \" +\n+        \"ON CONFLICT(day) DO UPDATE SET \" +\n+        \"raw_bytes = raw_bytes + excluded.raw_bytes, \" +\n+        \"compressed_bytes = excluded.compressed_bytes, \" +\n+        \"sqlite_growth_bytes = excluded.sqlite_growth_bytes, \" +\n+        \"blocks_ingested = blocks_ingested + excluded.blocks_ingested, \" +\n+        \"errors = errors + excluded.errors\"\n+    );\n   }\n@@\n   upsertTx(row: TxRow): void {\n     this.upsertTxStmt.run(row.tx_hash, row.block_number, row.tx_index);\n   }\n+\n+  addMetrics(day: number, rawBytes: number, blocksIngested: number, errors: number): void {\n+    this.upsertMetricsStmt.run(day, rawBytes, null, null, blocksIngested, errors);\n+  }\n*** End Patch"}}
  }

  close(): void {
    this.db.close();
  }

  getCursor(): Cursor | null {
    const row = this.getMetaStmt.get("cursor");
    const value = readValueString(row);
    if (!value) {
      return null;
    }
    return cursorFromJson(value);
  }

  setCursor(cursor: Cursor): void {
    const text = cursorToJson(cursor);
    this.upsertMetaStmt.run("cursor", text);
  }

  setMeta(key: string, value: string): void {
    this.upsertMetaStmt.run(key, value);
  }

  upsertBlock(row: BlockRow): void {
    this.upsertBlockStmt.run(row.number, row.hash, row.timestamp, row.tx_count);
  }

  upsertTx(row: TxRow): void {
    this.upsertTxStmt.run(row.tx_hash, row.block_number, row.tx_index);
  }

  transaction<T>(fn: () => T): T {
    return this.db.transaction(fn).immediate();
  }
}

function readValueString(row: unknown): string | null {
  if (!isRecord(row)) {
    return null;
  }
  const value = row.value;
  if (typeof value === "string") {
    return value;
  }
  if (value instanceof Buffer) {
    return value.toString("utf8");
  }
  return null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function schemaSql(): string {
  return `
    create table if not exists meta (
      key text primary key,
      value blob
    );

    create table if not exists blocks (
      number integer primary key,
      hash blob,
      timestamp integer not null,
      tx_count integer not null
    );

    create table if not exists txs (
      tx_hash blob primary key,
      block_number integer not null,
      tx_index integer not null
    );

    create table if not exists metrics_daily (
      day integer primary key,
      raw_bytes integer,
      compressed_bytes integer,
      sqlite_growth_bytes integer,
      blocks_ingested integer,
      errors integer
    );
  `;
}
