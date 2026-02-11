/// <reference path="../src/globals.d.ts" />
// どこで: indexerテスト / 何を: Postgres化後の主要ロジックを検証 / なぜ: SQLite撤去後の退行を防ぐため

import assert from "node:assert/strict";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { newDb } from "pg-mem";
import { cursorFromJson, cursorToJson } from "../src/cursor";
import { decodeTxIndexPayload } from "../src/decode";
import { archiveBlock } from "../src/archiver";
import { runArchiveGc, runArchiveGcWithMode } from "../src/archive_gc";
import { IndexerDb } from "../src/db";
import { MIGRATIONS } from "../src/migrations";
import { classifyExportError } from "../src/worker_errors";
import type { ExportError } from "../src/types";

type TestFn = () => void | Promise<void>;
type TestCase = { name: string; fn: TestFn };

const tests: TestCase[] = [];

function test(name: string, fn: TestFn): void {
  tests.push({ name, fn });
}

test("cursor json roundtrip", () => {
  const cursor = { block_number: 123n, segment: 1, byte_offset: 42 };
  const json = cursorToJson(cursor);
  const parsed = cursorFromJson(json);
  assert.equal(parsed.block_number, cursor.block_number);
  assert.equal(parsed.segment, cursor.segment);
  assert.equal(parsed.byte_offset, cursor.byte_offset);
});

test("tx_index payload length mismatch throws", () => {
  const txHash = Buffer.alloc(32, 0xaa);
  const len = Buffer.alloc(4);
  len.writeUInt32BE(8, 0);
  const payload = Buffer.concat([txHash, len, Buffer.alloc(8)]);
  assert.throws(() => decodeTxIndexPayload(payload), /entry size mismatch/);
});

test("archiveBlock reuses existing file", async () => {
  await withTempDir(async (dir) => {
    const input = {
      archiveDir: dir,
      chainId: "local",
      blockNumber: 1n,
      blockPayload: buildBlockPayload(1n, 10n, []),
      receiptsPayload: Buffer.alloc(0),
      txIndexPayload: Buffer.alloc(0),
      zstdLevel: 1,
    };
    const first = await archiveBlock(input);
    const second = await archiveBlock(input);
    assert.equal(first.path, second.path);
    assert.equal(first.sizeBytes, second.sizeBytes);
    assert.equal(first.sha256.toString("hex"), second.sha256.toString("hex"));
  });
});

test("db upsert and metrics aggregation", async () => {
  const db = await createTestIndexerDb();
  try {
    await db.upsertBlock({ number: 10n, hash: Buffer.alloc(32, 1), timestamp: 123n, tx_count: 1 });
    await db.upsertTx({ tx_hash: Buffer.alloc(32, 2), block_number: 10n, tx_index: 0 });
    await db.setCursor({ block_number: 11n, segment: 0, byte_offset: 0 });
    const cursor = await db.getCursor();
    assert.ok(cursor);
    assert.equal(cursor?.block_number, 11n);
    await db.addMetrics(20250101, 10, 5, 1, 0, 20);
    await db.addMetrics(20250101, 1, 1, 1, 0);
    const archiveSum0 = await db.getArchiveBytesSum();
    assert.equal(archiveSum0, 0);
    await db.addArchive({
      blockNumber: 10n,
      path: "10.bundle.zst",
      sha256: Buffer.alloc(32, 3),
      sizeBytes: 40,
      rawBytes: 50,
      createdAt: Date.now(),
    });
    const archiveSum = await db.getArchiveBytesSum();
    assert.equal(archiveSum, 40);
  } finally {
    await db.close();
  }
});

test("archive_gc keeps orphan by default and can remove with explicit mode", async () => {
  const db = await createTestIndexerDb();
  await withTempDir(async (dir) => {
    try {
      const root = path.join(dir, "local");
      await fs.mkdir(root, { recursive: true });
      const keepPath = path.join(root, "3.bundle.zst");
      const dropPath = path.join(root, "4.bundle.zst");
      await fs.writeFile(keepPath, Buffer.from("keep"));
      await fs.writeFile(dropPath, Buffer.from("drop"));
      await db.addArchive({
        blockNumber: 3n,
        path: "3.bundle.zst",
        sha256: Buffer.alloc(32, 1),
        sizeBytes: 4,
        rawBytes: 4,
        createdAt: Date.now(),
      });
      await runArchiveGc(db, dir, "local");
      assert.equal(await exists(keepPath), true);
      assert.equal(await exists(dropPath), true);
      await runArchiveGcWithMode(db, dir, "local", true);
      assert.equal(await exists(dropPath), false);
    } finally {
      await db.close();
    }
  });
});

test("retention cleanup dry-run and delete follow 90-day boundary", async () => {
  if (!process.env.TEST_INDEXER_DATABASE_URL) {
    process.stderr.write("[test] skip: retention cleanup dry-run and delete follow 90-day boundary (TEST_INDEXER_DATABASE_URL is not set)\n");
    return;
  }
  const db = await IndexerDb.connect({ databaseUrl: process.env.TEST_INDEXER_DATABASE_URL, poolMax: 2 });
  try {
    await db.queryOne("delete from txs");
    await db.queryOne("delete from blocks");
    await db.queryOne("delete from archive_parts");
    await db.queryOne("delete from metrics_daily");
    await db.queryOne("delete from retention_runs");
  } catch {
    // 初回は空でも問題なし
  }
  try {
    const nowSec = Math.floor(Date.now() / 1000);
    const oldSec = BigInt(nowSec - 91 * 24 * 60 * 60);
    const freshSec = BigInt(nowSec - 10 * 24 * 60 * 60);
    const oldDay = Number(formatDay(nowSec - 91 * 24 * 60 * 60));
    const freshDay = Number(formatDay(nowSec - 10 * 24 * 60 * 60));

    await db.upsertBlock({ number: 1n, hash: Buffer.alloc(32, 1), timestamp: oldSec, tx_count: 1 });
    await db.upsertBlock({ number: 2n, hash: Buffer.alloc(32, 2), timestamp: freshSec, tx_count: 1 });
    await db.upsertTx({ tx_hash: Buffer.alloc(32, 11), block_number: 1n, tx_index: 0 });
    await db.upsertTx({ tx_hash: Buffer.alloc(32, 22), block_number: 2n, tx_index: 0 });
    await db.addArchive({ blockNumber: 1n, path: "1.bundle.zst", sha256: Buffer.alloc(32, 3), sizeBytes: 10, rawBytes: 10, createdAt: Number(oldSec) * 1000 });
    await db.addArchive({ blockNumber: 2n, path: "2.bundle.zst", sha256: Buffer.alloc(32, 4), sizeBytes: 10, rawBytes: 10, createdAt: Number(freshSec) * 1000 });
    await db.addMetrics(oldDay, 1, 1, 1, 0, 10);
    await db.addMetrics(freshDay, 1, 1, 1, 0, 20);

    const dry = await db.runRetentionCleanup(90, true);
    assert.equal(dry.dry_run, true);
    assert.equal(Number(dry.deleted_blocks), 1);
    assert.equal(Number(dry.deleted_txs), 1);
    assert.equal(Number(dry.deleted_archive_parts), 1);

    const done = await db.runRetentionCleanup(90, false);
    assert.equal(done.dry_run, false);
    const blockCount = await db.queryOne<{ n: string }>("select count(*)::text as n from blocks");
    const txCount = await db.queryOne<{ n: string }>("select count(*)::text as n from txs");
    const archiveCount = await db.queryOne<{ n: string }>("select count(*)::text as n from archive_parts");
    const metricsCount = await db.queryOne<{ n: string }>("select count(*)::text as n from metrics_daily");
    assert.equal(Number(blockCount?.n ?? "0"), 1);
    assert.equal(Number(txCount?.n ?? "0"), 1);
    assert.equal(Number(archiveCount?.n ?? "0"), 1);
    assert.equal(Number(metricsCount?.n ?? "0"), 1);

    const latestRun = await db.queryOne<{ status: string }>("select status from retention_runs order by finished_at desc limit 1");
    assert.equal(latestRun?.status, "success");
  } finally {
    await db.close();
  }
});

test("classifyExportError updates metadata", async () => {
  const db = await createTestIndexerDb();
  try {
    const err: ExportError = { InvalidCursor: { message: "bad" } };
    const out = await classifyExportError(err, db);
    assert.equal(out.kind, "InvalidCursor");
    const lastError = await db.queryOne<{ value: string }>("select value from meta where key = $1", ["last_error"]);
    assert.equal(lastError?.value, "InvalidCursor");
  } finally {
    await db.close();
  }
});

async function createTestIndexerDb(): Promise<IndexerDb> {
  const mem = newDb({ noAstCoverageCheck: true });
  const adapter = mem.adapters.createPg();
  const pool = new adapter.Pool();
  const db = await IndexerDb.fromPool(pool, { migrations: MIGRATIONS.slice(0, 3) });
  await db.queryOne(
    "create table if not exists retention_runs(" +
      "id text primary key, started_at bigint not null, finished_at bigint not null, retention_days integer not null, dry_run boolean not null, deleted_blocks bigint not null, deleted_txs bigint not null, deleted_metrics_daily bigint not null, deleted_archive_parts bigint not null, status text not null, error_message text)"
  );
  return db;
}

async function run(): Promise<void> {
  const failures: string[] = [];
  for (const t of tests) {
    try {
      await t.fn();
      process.stderr.write(`[test] ok: ${t.name}\n`);
    } catch (err) {
      const detail = err instanceof Error ? err.message : String(err);
      process.stderr.write(`[test] fail: ${t.name}: ${detail}\n`);
      failures.push(t.name);
    }
  }
  if (failures.length > 0) {
    process.exit(1);
  }
}

run().catch((err) => {
  const detail = err instanceof Error ? err.message : String(err);
  process.stderr.write(`[test] fatal: ${detail}\n`);
  process.exit(1);
});

async function withTempDir(fn: (dir: string) => Promise<void>): Promise<void> {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "indexer-test-"));
  try {
    await fn(dir);
  } finally {
    await fs.rm(dir, { recursive: true, force: true });
  }
}

function buildBlockPayload(number: bigint, timestamp: bigint, txIds: Buffer[]): Buffer {
  const hashLen = 32;
  const base = 8 + hashLen + hashLen + 8 + hashLen + hashLen + 4;
  const total = base + txIds.length * hashLen;
  const out = Buffer.alloc(total);
  let offset = 0;
  offset = writeU64BE(out, offset, number);
  offset = writeZeros(out, offset, hashLen);
  offset = writeZeros(out, offset, hashLen);
  offset = writeU64BE(out, offset, timestamp);
  offset = writeZeros(out, offset, hashLen);
  offset = writeZeros(out, offset, hashLen);
  out.writeUInt32BE(txIds.length, offset);
  offset += 4;
  for (const txId of txIds) {
    txId.copy(out, offset);
    offset += hashLen;
  }
  return out;
}

function writeU64BE(buf: Buffer, offset: number, value: bigint): number {
  const high = Number((value >> 32n) & 0xffff_ffffn);
  const low = Number(value & 0xffff_ffffn);
  buf.writeUInt32BE(high, offset);
  buf.writeUInt32BE(low, offset + 4);
  return offset + 8;
}

function writeZeros(buf: Buffer, offset: number, len: number): number {
  buf.fill(0, offset, offset + len);
  return offset + len;
}

async function exists(filePath: string): Promise<boolean> {
  try {
    const stat = await fs.stat(filePath);
    return stat.isFile();
  } catch {
    return false;
  }
}

function formatDay(epochSec: number): string {
  const d = new Date(epochSec * 1000);
  const y = d.getUTCFullYear();
  const m = String(d.getUTCMonth() + 1).padStart(2, "0");
  const day = String(d.getUTCDate()).padStart(2, "0");
  return `${y}${m}${day}`;
}
