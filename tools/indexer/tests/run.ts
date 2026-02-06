/// <reference path="../src/globals.d.ts" />
// どこで: indexerテスト / 何を: ユニット/統合/疑似E2E / なぜ: ローカル検証を確実にするため

import assert from "node:assert/strict";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import Database from "better-sqlite3";
import { cursorFromJson, cursorToJson } from "../src/cursor";
import { decodeTxIndexPayload } from "../src/decode";
import { archiveBlock } from "../src/archiver";
import { runArchiveGc } from "../src/archive_gc";
import { IndexerDb } from "../src/db";
import { applyMigrations, MIGRATIONS } from "../src/migrations";
import { _test, runWorkerWithDeps } from "../src/worker";
import { Chunk, Cursor, ExportError, ExportResponse, PruneStatusView, Result } from "../src/types";

type TestFn = () => void | Promise<void>;
type TestCase = { name: string; fn: TestFn };

const tests: TestCase[] = [];

function test(name: string, fn: TestFn): void {
  tests.push({ name, fn });
}

function defaultPruneStatusView(): PruneStatusView {
  return {
    pruning_enabled: false,
    prune_running: false,
    estimated_kept_bytes: 0n,
    high_water_bytes: 0n,
    low_water_bytes: 0n,
    hard_emergency_bytes: 0n,
    last_prune_at: 0n,
    pruned_before_block: null,
    oldest_kept_block: null,
    oldest_kept_timestamp: null,
    need_prune: false,
  };
}

function okResult<T, E>(value: T): Result<T, E> {
  return { Ok: value };
}

function errResult<T, E>(err: E): Result<T, E> {
  return { Err: err };
}

test("cursor json roundtrip", () => {
  const cursor = { block_number: 123n, segment: 1, byte_offset: 42 };
  const json = cursorToJson(cursor);
  const parsed = cursorFromJson(json);
  assert.equal(parsed.block_number, cursor.block_number);
  assert.equal(parsed.segment, cursor.segment);
  assert.equal(parsed.byte_offset, cursor.byte_offset);
});

test("cursor invalid segment is rejected", () => {
  const bad = JSON.stringify({ v: 1, block_number: "1", segment: 3, byte_offset: 0 });
  assert.throws(() => cursorFromJson(bad), /segment/);
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

test("archive_gc keeps bundle when db is empty and removes tmp", async () => {
  await withTempDir(async (dir) => {
    const dbPath = path.join(dir, "db.sqlite");
    const db = new IndexerDb(dbPath);
    const root = path.join(dir, "local");
    await fs.mkdir(root, { recursive: true });
    const bundle = path.join(root, "1.bundle.zst");
    const tmp = path.join(root, "2.bundle.zst.tmp");
    await fs.writeFile(bundle, Buffer.from("bundle"));
    await fs.writeFile(tmp, Buffer.from("tmp"));
    await runArchiveGc(db, dir, "local");
    assert.equal(await exists(bundle), true);
    assert.equal(await exists(tmp), false);
    db.close();
  });
});

test("archive_gc preserves referenced relative path and removes orphan", async () => {
  await withTempDir(async (dir) => {
    const dbPath = path.join(dir, "db.sqlite");
    const db = new IndexerDb(dbPath);
    const root = path.join(dir, "local");
    await fs.mkdir(root, { recursive: true });
    const keepPath = path.join(root, "3.bundle.zst");
    const dropPath = path.join(root, "4.bundle.zst");
    await fs.writeFile(keepPath, Buffer.from("keep"));
    await fs.writeFile(dropPath, Buffer.from("drop"));
    db.addArchive({
      blockNumber: 3n,
      path: "3.bundle.zst",
      sha256: Buffer.alloc(32, 1),
      sizeBytes: 4,
      rawBytes: 4,
      createdAt: Date.now(),
    });
    await runArchiveGc(db, dir, "local");
    assert.equal(await exists(keepPath), true);
    assert.equal(await exists(dropPath), false);
    db.close();
  });
});

test("migrations are idempotent", async () => {
  await withTempDir(async (dir) => {
    const dbPath = path.join(dir, "db.sqlite");
    const db = new Database(dbPath);
    applyMigrations(db, MIGRATIONS);
    applyMigrations(db, MIGRATIONS);
    const count = getCount(db, "schema_migrations");
    assert.equal(count, MIGRATIONS.length);
    db.close();
  });
});

test("pseudo-e2e: archive + db + gc keeps file", async () => {
  await withTempDir(async (dir) => {
    const dbPath = path.join(dir, "db.sqlite");
    const db = new IndexerDb(dbPath);
    const input = {
      archiveDir: dir,
      chainId: "local",
      blockNumber: 9n,
      blockPayload: buildBlockPayload(9n, 99n, []),
      receiptsPayload: Buffer.alloc(0),
      txIndexPayload: Buffer.alloc(0),
      zstdLevel: 1,
    };
    const archive = await archiveBlock(input);
    db.addArchive({
      blockNumber: 9n,
      path: path.relative(path.join(dir, "local"), archive.path),
      sha256: archive.sha256,
      sizeBytes: archive.sizeBytes,
      rawBytes: archive.rawBytes,
      createdAt: Date.now(),
    });
    await runArchiveGc(db, dir, "local");
    assert.equal(await exists(archive.path), true);
    db.close();
  });
});

test("enforceNextCursor and applyChunks invalid cases throw", () => {
  const cursor: Cursor = { block_number: 10n, segment: 0, byte_offset: 0 };
  const badChunk: Chunk = { segment: 1, start: 0, payload_len: 10, bytes: Buffer.alloc(1) };
  const responseBad: ExportResponse = { chunks: [badChunk], next_cursor: cursor };
  assert.throws(() => _test.enforceNextCursor(responseBad, cursor));

  const pending = _test.newPending(cursor);
  assert.throws(() => _test.applyChunks(pending, [badChunk], cursor));

  const okChunk: Chunk = { segment: 0, start: 0, payload_len: 4, bytes: Buffer.alloc(4) };
  const okNext: Cursor = { block_number: 10n, segment: 1, byte_offset: 0 };
  _test.enforceNextCursor({ chunks: [okChunk], next_cursor: okNext }, cursor);

  const pendingOk = _test.newPending(cursor);
  _test.applyChunks(pendingOk, [okChunk], cursor);
  assert.equal(pendingOk.segment, 1);

  const badLenChunk: Chunk = { segment: 1, start: 0, payload_len: 2, bytes: Buffer.alloc(3) };
  assert.throws(() => _test.applyChunks(pendingOk, [badLenChunk], cursor));
});

test("max_bytes over limit triggers fatal exit", async () => {
  await withTempDir(async (dir) => {
    const db = new IndexerDb(path.join(dir, "db.sqlite"));
    const cursor: Cursor = { block_number: 1n, segment: 0, byte_offset: 0 };
    const chunks: Chunk[] = [
      { segment: 0, start: 0, payload_len: 1, bytes: Buffer.alloc(2) },
    ];
    const response: ExportResponse = { chunks, next_cursor: cursor };
    const client = {
      getHeadNumber: async () => 1n,
      exportBlocks: async () => okResult<ExportResponse, ExportError>(response),
      getPruneStatus: async () => defaultPruneStatusView(),
    };
    const config = {
      canisterId: "x",
      icHost: "http://localhost",
      dbPath: path.join(dir, "db.sqlite"),
      maxBytes: 1,
      backoffInitialMs: 1,
      backoffMaxMs: 1,
      idlePollMs: 1,
      pruneStatusPollMs: 0,
      fetchRootKey: false,
      archiveDir: dir,
      chainId: "local",
      zstdLevel: 1,
    };
    await expectExit(async () => {
      await runWorkerWithDeps(config, db, client, { skipGc: true });
    });
    db.close();
  });
});

test("Pruned error triggers fatal exit", async () => {
  await withTempDir(async (dir) => {
    const db = new IndexerDb(path.join(dir, "db.sqlite"));
    const cursor: Cursor = { block_number: 1n, segment: 0, byte_offset: 0 };
    const err: ExportError = { Pruned: { pruned_before_block: 1n } };
    const client = {
      getHeadNumber: async () => 1n,
      exportBlocks: async () => errResult<ExportResponse, ExportError>(err),
      getPruneStatus: async () => defaultPruneStatusView(),
    };
    const config = {
      canisterId: "x",
      icHost: "http://localhost",
      dbPath: path.join(dir, "db.sqlite"),
      maxBytes: 10,
      backoffInitialMs: 1,
      backoffMaxMs: 1,
      idlePollMs: 1,
      pruneStatusPollMs: 0,
      fetchRootKey: false,
      archiveDir: dir,
      chainId: "local",
      zstdLevel: 1,
    };
    db.setCursor(cursor);
    await expectExit(async () => {
      await runWorkerWithDeps(config, db, client, { skipGc: true });
    });
    db.close();
  });
});

test("prune_status is persisted as JSON with string fields", async () => {
  await withTempDir(async (dir) => {
    const dbPath = path.join(dir, "db.sqlite");
    const db = new IndexerDb(dbPath);
    const cursor: Cursor = { block_number: 1n, segment: 0, byte_offset: 0 };
    const err: ExportError = { Pruned: { pruned_before_block: 1n } };
    const client = {
      getHeadNumber: async () => 1n,
      exportBlocks: async () => errResult<ExportResponse, ExportError>(err),
      getPruneStatus: async () => ({
        pruning_enabled: true,
        prune_running: false,
        estimated_kept_bytes: 123n,
        high_water_bytes: 456n,
        low_water_bytes: 400n,
        hard_emergency_bytes: 900n,
        last_prune_at: 10n,
        pruned_before_block: 5n,
        oldest_kept_block: 6n,
        oldest_kept_timestamp: 7n,
        need_prune: true,
      }),
    };
    const config = {
      canisterId: "x",
      icHost: "http://localhost",
      dbPath,
      maxBytes: 10,
      backoffInitialMs: 1,
      backoffMaxMs: 1,
      idlePollMs: 1,
      pruneStatusPollMs: 1,
      fetchRootKey: false,
      archiveDir: dir,
      chainId: "local",
      zstdLevel: 1,
    };
    db.setCursor(cursor);
    await expectExit(async () => {
      await runWorkerWithDeps(config, db, client, { skipGc: true });
    });
    db.close();
    const raw = readMetaValue(dbPath, "prune_status");
    assert.ok(raw, "prune_status is missing");
    if (typeof raw !== "string") {
      throw new Error("prune_status is not a string");
    }
    const parsed = JSON.parse(raw);
    assert.equal(parsed.v, 1);
    assert.equal(parsed.status.estimated_kept_bytes, "123");
    assert.equal(parsed.status.high_water_bytes, "456");
    assert.equal(parsed.status.pruned_before_block, "5");
  });
});

test("sqlite_bytes and archive_bytes are updated once per day", async () => {
  await withTempDir(async (dir) => {
    const dbPath = path.join(dir, "db.sqlite");
    const db = new IndexerDb(dbPath);
    const payload1 = buildBlockPayload(1n, 100n, [Buffer.alloc(32, 1)]);
    const payload2 = buildBlockPayload(2n, 101n, [Buffer.alloc(32, 2)]);
    const txIndex1 = buildTxIndexPayload(1n, 0, Buffer.alloc(32, 1));
    const txIndex2 = buildTxIndexPayload(2n, 0, Buffer.alloc(32, 2));
    const receipts = Buffer.alloc(0);
    const responses: ExportResponse[] = [
      buildResponseFromPayloads(1n, payload1, receipts, txIndex1),
      buildResponseFromPayloads(2n, payload2, receipts, txIndex2),
    ];
    let idx = 0;
    const client = {
      getHeadNumber: async () => 2n,
      exportBlocks: async () => {
        if (idx < responses.length) {
          const value = responses[idx];
          idx += 1;
          return okResult<ExportResponse, ExportError>(value);
        }
        return errResult<ExportResponse, ExportError>({ Pruned: { pruned_before_block: 0n } });
      },
      getPruneStatus: async () => ({
        pruning_enabled: false,
        prune_running: false,
        estimated_kept_bytes: 0n,
        high_water_bytes: 0n,
        low_water_bytes: 0n,
        hard_emergency_bytes: 0n,
        last_prune_at: 0n,
        pruned_before_block: null,
        oldest_kept_block: null,
        oldest_kept_timestamp: null,
        need_prune: false,
      }),
    };
    const config = {
      canisterId: "x",
      icHost: "http://localhost",
      dbPath,
      maxBytes: 1_000_000,
      backoffInitialMs: 1,
      backoffMaxMs: 1,
      idlePollMs: 1,
      pruneStatusPollMs: 0,
      fetchRootKey: false,
      archiveDir: dir,
      chainId: "local",
      zstdLevel: 1,
    };
    await expectExit(async () => {
      await runWorkerWithDeps(config, db, client, { skipGc: true });
    });
    db.close();
    const row = readMetricsRow(dbPath);
    assert.ok(typeof row.sqlite_bytes === "number", "sqlite_bytes missing");
    assert.ok(typeof row.archive_bytes === "number", "archive_bytes missing");
    const archiveSum = readArchiveSum(dbPath);
    assert.ok(row.archive_bytes <= archiveSum, "archive_bytes should not exceed current sum");
  });
});

test("metrics sqlite/archive bytes keep first value within a day", async () => {
  await withTempDir(async (dir) => {
    const dbPath = path.join(dir, "db.sqlite");
    const db = new IndexerDb(dbPath);
    const day = 20250101;
    db.addMetrics(day, 10, 5, 1, 0, 100, 200);
    db.addMetrics(day, 1, 1, 1, 0);
    db.close();
    const row = readMetricsRow(dbPath);
    assert.equal(row.sqlite_bytes, 100);
    assert.equal(row.archive_bytes, 200);
  });
});

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

function getCount(db: Database.Database, table: string): number {
  const row = db.prepare(`select count(*) as n from ${table}`).get();
  if (!isRecord(row) || typeof row.n !== "number") {
    throw new Error("count query failed");
  }
  return row.n;
}

async function expectExit(fn: () => Promise<void>): Promise<void> {
  const originalExit = process.exit;
  const originalWrite = process.stderr.write;
  const logs: string[] = [];
  Object.defineProperty(process.stderr, "write", {
    value: (chunk: string) => {
      logs.push(chunk);
      return true;
    },
    configurable: true,
  });
  Object.defineProperty(process, "exit", {
    value: (code?: number) => {
      const msg = `exit:${code ?? 0}`;
      throw new Error(msg);
    },
    configurable: true,
  });
  try {
    await fn();
    throw new Error("expected exit");
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    if (!message.startsWith("exit:")) {
      throw err;
    }
    const fatal = logs.find((line) => line.includes("\"event\":\"fatal\""));
    assert.ok(fatal, "fatal log missing");
  } finally {
    Object.defineProperty(process, "exit", { value: originalExit, configurable: true });
    Object.defineProperty(process.stderr, "write", { value: originalWrite, configurable: true });
  }
}

function readMetaValue(dbPath: string, key: string): string | null {
  const db = new Database(dbPath);
  const row = db.prepare("select value from meta where key = ?").get(key);
  db.close();
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

function readMetricsRow(dbPath: string): { sqlite_bytes: number | null; archive_bytes: number | null } {
  const db = new Database(dbPath);
  const row = db.prepare("select sqlite_bytes, archive_bytes from metrics_daily limit 1").get();
  db.close();
  if (!isRecord(row)) {
    throw new Error("metrics row missing");
  }
  const sqliteBytes = typeof row.sqlite_bytes === "number" ? row.sqlite_bytes : null;
  const archiveBytes = typeof row.archive_bytes === "number" ? row.archive_bytes : null;
  return { sqlite_bytes: sqliteBytes, archive_bytes: archiveBytes };
}

function readArchiveSum(dbPath: string): number {
  const db = new Database(dbPath);
  const row = db.prepare("select coalesce(sum(size_bytes), 0) as total from archive_parts").get();
  db.close();
  if (!isRecord(row) || typeof row.total !== "number") {
    return 0;
  }
  return row.total;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function buildTxIndexPayload(blockNumber: bigint, txIndex: number, txHash: Buffer): Buffer {
  const entry = Buffer.alloc(12);
  entry.writeBigUInt64BE(blockNumber, 0);
  entry.writeUInt32BE(txIndex, 8);
  const len = Buffer.alloc(4);
  len.writeUInt32BE(entry.length, 0);
  return Buffer.concat([txHash, len, entry]);
}

function buildResponseFromPayloads(
  blockNumber: bigint,
  block: Buffer,
  receipts: Buffer,
  txIndex: Buffer
): ExportResponse {
  const chunks: Chunk[] = [
    { segment: 0, start: 0, payload_len: block.length, bytes: block },
    { segment: 1, start: 0, payload_len: receipts.length, bytes: receipts },
    { segment: 2, start: 0, payload_len: txIndex.length, bytes: txIndex },
  ];
  const next: Cursor = { block_number: blockNumber + 1n, segment: 0, byte_offset: 0 };
  return { chunks, next_cursor: next };
}
