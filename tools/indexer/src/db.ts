// どこで: Postgresアクセス / 何を: スキーマとUPSERT / なぜ: コミット境界を守るため

import { Pool, type PoolClient } from "pg";
import type { QueryResultRow } from "pg";
import { applyMigrations, MIGRATIONS } from "./migrations";
import { cursorFromJson, cursorToJson } from "./cursor";
import type { Cursor } from "./types";

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

export type RetentionCleanupResult = {
  run_id: string;
  started_at_ms: bigint;
  finished_at_ms: bigint;
  retention_days: number;
  dry_run: boolean;
  deleted_blocks: bigint;
  deleted_txs: bigint;
  deleted_metrics_daily: bigint;
  deleted_archive_parts: bigint;
  status: string;
  error_message: string | null;
};

export class IndexerDb {
  private readonly pool: Pool;

  private constructor(pool: Pool) {
    this.pool = pool;
  }

  static async connect(params: { databaseUrl: string; poolMax: number }): Promise<IndexerDb> {
    const pool = new Pool({ connectionString: params.databaseUrl, max: params.poolMax });
    await applyMigrations(pool, MIGRATIONS);
    return new IndexerDb(pool);
  }

  static async fromPool(pool: Pool, options?: { migrations?: readonly string[] }): Promise<IndexerDb> {
    await applyMigrations(pool, options?.migrations ?? MIGRATIONS);
    return new IndexerDb(pool);
  }

  async close(): Promise<void> {
    await this.pool.end();
  }

  async ensureRetentionSchedule(retentionDays: number): Promise<void> {
    try {
      const hasCron = await this.pool.query<{ extname: string }>(
        "select extname from pg_extension where extname = 'pg_cron' limit 1"
      );
      if (hasCron.rowCount === 0) {
        return;
      }

      const hasJobTable = await this.pool.query<{ exists: boolean }>(
        "select exists(select 1 from information_schema.tables where table_schema = 'cron' and table_name = 'job') as exists"
      );
      if (!hasJobTable.rows[0]?.exists) {
        return;
      }

      const jobName = "ic_op_retention_daily";
      const existing = await this.pool.query<{ jobid: number }>("select jobid from cron.job where jobname = $1 limit 1", [
        jobName,
      ]);
      if ((existing.rowCount ?? 0) > 0) {
        return;
      }

      await this.pool.query(
        "select cron.schedule($1, $2, $3)",
        [jobName, "17 2 * * *", `select * from run_retention_cleanup(${retentionDays}, false)`]
      );
    } catch {
      // pg_cron 未導入や権限不足は非致命として無視する
    }
  }

  async runRetentionCleanup(retentionDays: number, dryRun: boolean): Promise<RetentionCleanupResult> {
    const row = await this.pool.query<RetentionCleanupResult>(
      "select * from run_retention_cleanup($1, $2)",
      [retentionDays, dryRun]
    );
    const result = row.rows[0];
    if (!result) {
      throw new Error("run_retention_cleanup returned no rows");
    }
    return result;
  }

  async getCursor(): Promise<Cursor | null> {
    const row = await this.pool.query<{ value: string | Buffer }>("SELECT value FROM meta WHERE key = $1", ["cursor"]);
    if (row.rowCount === 0) {
      return null;
    }
    const value = row.rows[0]?.value;
    if (!value) {
      return null;
    }
    const text = typeof value === "string" ? value : value.toString("utf8");
    return cursorFromJson(text);
  }

  async setCursor(cursor: Cursor): Promise<void> {
    const text = cursorToJson(cursor);
    await this.setMeta("cursor", text);
  }

  async setMeta(key: string, value: string): Promise<void> {
    await this.pool.query(
      "INSERT INTO meta(key, value) VALUES($1, $2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
      [key, value]
    );
  }

  async upsertBlock(row: BlockRow): Promise<void> {
    await this.pool.query(
      "INSERT INTO blocks(number, hash, timestamp, tx_count) VALUES($1, $2, $3, $4) ON CONFLICT(number) DO UPDATE SET hash = excluded.hash, timestamp = excluded.timestamp, tx_count = excluded.tx_count",
      [row.number, row.hash, row.timestamp, row.tx_count]
    );
  }

  async upsertTx(row: TxRow): Promise<void> {
    await this.pool.query(
      "INSERT INTO txs(tx_hash, block_number, tx_index) VALUES($1, $2, $3) ON CONFLICT(tx_hash) DO UPDATE SET block_number = excluded.block_number, tx_index = excluded.tx_index",
      [row.tx_hash, row.block_number, row.tx_index]
    );
  }

  async addMetrics(
    day: number,
    rawBytes: number,
    compressedBytes: number,
    blocksIngested: number,
    errors: number,
    archiveBytes: number | null = null
  ): Promise<void> {
    await this.pool.query(
      "INSERT INTO metrics_daily(day, raw_bytes, compressed_bytes, archive_bytes, blocks_ingested, errors) VALUES($1, $2, $3, $4, $5, $6) " +
        "ON CONFLICT(day) DO UPDATE SET " +
        "raw_bytes = metrics_daily.raw_bytes + excluded.raw_bytes, " +
        "compressed_bytes = metrics_daily.compressed_bytes + excluded.compressed_bytes, " +
        "archive_bytes = COALESCE(excluded.archive_bytes, metrics_daily.archive_bytes), " +
        "blocks_ingested = metrics_daily.blocks_ingested + excluded.blocks_ingested, " +
        "errors = metrics_daily.errors + excluded.errors",
      [day, rawBytes, compressedBytes, archiveBytes, blocksIngested, errors]
    );
  }

  async addArchive(params: {
    blockNumber: bigint;
    path: string;
    sha256: Buffer;
    sizeBytes: number;
    rawBytes: number;
    createdAt: number;
  }): Promise<void> {
    await this.pool.query(
      "INSERT INTO archive_parts(block_number, path, sha256, size_bytes, raw_bytes, created_at) VALUES($1, $2, $3, $4, $5, $6) " +
        "ON CONFLICT(block_number) DO UPDATE SET path = excluded.path, sha256 = excluded.sha256, size_bytes = excluded.size_bytes, raw_bytes = excluded.raw_bytes, created_at = excluded.created_at",
      [params.blockNumber, params.path, params.sha256, params.sizeBytes, params.rawBytes, params.createdAt]
    );
  }

  async transaction<T>(fn: (client: PoolClient) => Promise<T>): Promise<T> {
    const client = await this.pool.connect();
    try {
      await client.query("BEGIN");
      const result = await fn(client);
      await client.query("COMMIT");
      return result;
    } catch (err) {
      await client.query("ROLLBACK");
      throw err;
    } finally {
      client.release();
    }
  }

  async listArchivePaths(): Promise<Set<string>> {
    const rows = await this.pool.query<{ path: string }>("select path from archive_parts");
    return new Set(rows.rows.map((row) => row.path));
  }

  async getArchiveBytesSum(): Promise<number> {
    const row = await this.pool.query<{ total: string | number }>("select coalesce(sum(size_bytes), 0) as total from archive_parts");
    const value = row.rows[0]?.total;
    if (typeof value === "number") {
      return value;
    }
    return Number(value ?? 0);
  }

  async queryOne<T extends QueryResultRow>(sql: string, params: unknown[] = []): Promise<T | null> {
    const out = await this.pool.query<T>(sql, params);
    return out.rows[0] ?? null;
  }
}
