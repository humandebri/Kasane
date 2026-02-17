/// <reference path="../src/globals.d.ts" />
// どこで: indexerテスト / 何を: Postgres化後の主要ロジックを検証 / なぜ: SQLite撤去後の退行を防ぐため

import assert from "node:assert/strict";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { newDb } from "pg-mem";
import { cursorFromJson, cursorToJson } from "../src/cursor";
import { clientTestHooks } from "../src/client";
import { decodeBlockPayload, decodeReceiptStatusPayload, decodeTxIndexPayload } from "../src/decode";
import { archiveBlock } from "../src/archiver";
import { runArchiveGc, runArchiveGcWithMode } from "../src/archive_gc";
import { IndexerDb } from "../src/db";
import { MIGRATIONS } from "../src/migrations";
import { applyChunk, enforceNextCursor, finalizePayloads, newPendingFromChunk } from "../src/worker_pending";
import { runWorkerWithDeps } from "../src/worker";
import { classifyExportError } from "../src/worker_errors";
import type { ExportError } from "../src/types";
import type { Config } from "../src/config";

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

test("cursor json allows segment above legacy cap", () => {
  const parsed = cursorFromJson(
    JSON.stringify({ v: 1, block_number: "123", segment: 3, byte_offset: 0 })
  );
  assert.equal(parsed.segment, 3);
});

test("cursor json rejects negative segment", () => {
  assert.throws(
    () => cursorFromJson(JSON.stringify({ v: 1, block_number: "123", segment: -1, byte_offset: 0 })),
    /cursor.segment out of range/
  );
});

test("client cursor normalization accepts numeric strings", () => {
  const out = clientTestHooks.normalizeCursorForCandid({
    block_number: "1",
    segment: "0",
    byte_offset: "0",
  });
  assert.equal(out.block_number, 1n);
  assert.equal(out.segment, 0);
  assert.equal(out.byte_offset, 0);
});

test("tx_index payload length mismatch throws", () => {
  const txHash = Buffer.alloc(32, 0xaa);
  const len = Buffer.alloc(4);
  len.writeUInt32BE(8, 0);
  const payload = Buffer.concat([txHash, len, Buffer.alloc(8)]);
  assert.throws(() => decodeTxIndexPayload(payload), /entry size mismatch/);
});

test("tx_index payload rejects legacy 12-byte entry", () => {
  const txHash = Buffer.alloc(32, 0xbb);
  const len = Buffer.alloc(4);
  len.writeUInt32BE(12, 0);
  const body = Buffer.alloc(12);
  const payload = Buffer.concat([txHash, len, body]);
  assert.throws(() => decodeTxIndexPayload(payload), /35\+ bytes/);
});

test("tx_index payload decodes caller principal", () => {
  const txHash = Buffer.alloc(32, 0xcc);
  const principal = Buffer.from([1, 2, 3, 4]);
  const fromAddress = Buffer.alloc(20, 0x11);
  const toAddress = Buffer.alloc(20, 0x22);
  const body = Buffer.alloc(12 + 2 + principal.length + 20 + 1 + toAddress.length);
  body.writeUInt32BE(0, 0);
  body.writeUInt32BE(7, 4);
  body.writeUInt32BE(3, 8);
  body.writeUInt16BE(principal.length, 12);
  principal.copy(body, 14);
  fromAddress.copy(body, 14 + principal.length);
  body.writeUInt8(toAddress.length, 14 + principal.length + fromAddress.length);
  toAddress.copy(body, 14 + principal.length + fromAddress.length + 1);
  const len = Buffer.alloc(4);
  len.writeUInt32BE(body.length, 0);
  const payload = Buffer.concat([txHash, len, body]);
  const out = decodeTxIndexPayload(payload);
  assert.equal(out.length, 1);
  assert.equal(out[0]?.blockNumber, 7n);
  assert.equal(out[0]?.txIndex, 3);
  assert.equal(out[0]?.callerPrincipal?.toString("hex"), principal.toString("hex"));
  assert.equal(out[0]?.fromAddress.toString("hex"), fromAddress.toString("hex"));
  assert.equal(out[0]?.toAddress?.toString("hex"), toAddress.toString("hex"));
});

test("receipts payload decodes status", () => {
  const txHash = Buffer.alloc(32, 0x33);
  const receipt = buildReceiptBytes(1, true);
  const len = Buffer.alloc(4);
  len.writeUInt32BE(receipt.length, 0);
  const payload = Buffer.concat([txHash, len, receipt]);
  const out = decodeReceiptStatusPayload(payload);
  assert.equal(out.length, 1);
  assert.equal(out[0]?.txHash.toString("hex"), txHash.toString("hex"));
  assert.equal(out[0]?.status, 1);
});

test("receipts payload rejects invalid status", () => {
  const txHash = Buffer.alloc(32, 0x44);
  const receipt = buildReceiptBytes(2, true);
  const len = Buffer.alloc(4);
  len.writeUInt32BE(receipt.length, 0);
  const payload = Buffer.concat([txHash, len, receipt]);
  assert.throws(() => decodeReceiptStatusPayload(payload), /status/);
});

test("block payload decodes v2 layout", () => {
  const number = Buffer.alloc(8);
  number.writeBigUInt64BE(7n, 0);
  const parentHash = Buffer.alloc(32, 0x01);
  const blockHash = Buffer.alloc(32, 0xaa);
  const timestamp = Buffer.alloc(8);
  timestamp.writeBigUInt64BE(123n, 0);
  const baseFee = Buffer.alloc(8);
  baseFee.writeBigUInt64BE(250_000_000_000n, 0);
  const blockGasLimit = Buffer.alloc(8);
  blockGasLimit.writeBigUInt64BE(3_000_000n, 0);
  const gasUsed = Buffer.alloc(8);
  gasUsed.writeBigUInt64BE(21_000n, 0);
  const txListHash = Buffer.alloc(32, 0x02);
  const stateRoot = Buffer.alloc(32, 0x03);
  const txLen = Buffer.alloc(4);
  txLen.writeUInt32BE(1, 0);
  const txId = Buffer.alloc(32, 0xbb);
  const payload = Buffer.concat([
    number,
    parentHash,
    blockHash,
    timestamp,
    baseFee,
    blockGasLimit,
    gasUsed,
    txListHash,
    stateRoot,
    txLen,
    txId,
  ]);
  const out = decodeBlockPayload(payload);
  assert.equal(out.number, 7n);
  assert.equal(out.timestamp, 123n);
  assert.equal(out.blockHash.toString("hex"), blockHash.toString("hex"));
  assert.equal(out.txIds.length, 1);
  assert.equal(out.txIds[0]?.toString("hex"), txId.toString("hex"));
});

test("enforceNextCursor allows same-block forward progress", () => {
  const cursor = { block_number: 10n, segment: 1, byte_offset: 40 };
  const response = {
    chunks: [{ segment: 1, start: 40, bytes: Buffer.from([1, 2]), payload_len: 200 }],
    next_cursor: { block_number: 10n, segment: 1, byte_offset: 42 },
  };
  enforceNextCursor(response, cursor, 2);
});

test("enforceNextCursor allows block jump when non-regressive", () => {
  const cursor = { block_number: 10n, segment: 2, byte_offset: 100 };
  const response = {
    chunks: [{ segment: 2, start: 100, bytes: Buffer.from([1]), payload_len: 101 }],
    next_cursor: { block_number: 12n, segment: 0, byte_offset: 0 },
  };
  enforceNextCursor(response, cursor, 2);
});

test("enforceNextCursor allows non-zero segment on block increment", () => {
  const cursor = { block_number: 10n, segment: 2, byte_offset: 100 };
  const response = {
    chunks: [{ segment: 2, start: 100, bytes: Buffer.from([1]), payload_len: 101 }],
    next_cursor: { block_number: 11n, segment: 1, byte_offset: 0 },
  };
  enforceNextCursor(response, cursor, 2);
});

test("enforceNextCursor rejects out-of-range segment with schema hint", () => {
  const cursor = { block_number: 10n, segment: 2, byte_offset: 100 };
  const response = {
    chunks: [{ segment: 2, start: 100, bytes: Buffer.from([1]), payload_len: 101 }],
    next_cursor: { block_number: 11n, segment: 3, byte_offset: 0 },
  };
  assert.throws(() => enforceNextCursor(response, cursor, 2), /segment schema mismatch/);
});

test("enforceNextCursor respects configurable maxSegment", () => {
  const cursor = { block_number: 10n, segment: 2, byte_offset: 100 };
  const response = {
    chunks: [{ segment: 2, start: 100, bytes: Buffer.from([1]), payload_len: 101 }],
    next_cursor: { block_number: 11n, segment: 3, byte_offset: 0 },
  };
  enforceNextCursor(response, cursor, 3);
});

test("applyChunk can process multiple blocks in one chunk stream", () => {
  const block1 = buildBlockPayload(1n, 10n, []);
  const block2 = buildBlockPayload(2n, 20n, []);
  const chunks = [
    { segment: 0, start: 0, bytes: block1, payload_len: block1.length },
    { segment: 1, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
    { segment: 2, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
    { segment: 0, start: 0, bytes: block2, payload_len: block2.length },
    { segment: 1, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
    { segment: 2, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
  ];
  let pending: ReturnType<typeof newPendingFromChunk> | null = newPendingFromChunk(chunks[0]);
  const seen: bigint[] = [];
  for (let i = 0; i < chunks.length; i += 1) {
    if (!pending) {
      pending = newPendingFromChunk(chunks[i]);
    }
    applyChunk(pending, chunks[i]);
    if (pending.complete) {
      const payloads = finalizePayloads(pending);
      seen.push(decodeBlockPayload(payloads[0]).number);
      pending = null;
    }
  }
  assert.deepEqual(seen, [1n, 2n]);
});

test("runWorkerWithDeps commits two blocks from one response and stores final cursor", async () => {
  const db = await createTestIndexerDb();
  const originalClose = db.close.bind(db);
  (db as unknown as { close: () => Promise<void> }).close = async () => {};
  await withTempDir(async (dir) => {
    const config: Config = {
      canisterId: "test-canister",
      icHost: "http://127.0.0.1:4943",
      databaseUrl: "postgres://unused",
      dbPoolMax: 1,
      retentionDays: 90,
      retentionEnabled: false,
      retentionDryRun: false,
      archiveGcDeleteOrphans: false,
      maxBytes: 1_200_000,
      backoffInitialMs: 1,
      backoffMaxMs: 2,
      idlePollMs: 1,
      pruneStatusPollMs: 0,
      opsMetricsPollMs: 0,
      fetchRootKey: false,
      archiveDir: dir,
      chainId: "test",
      zstdLevel: 1,
      maxSegment: 2,
    };
    await db.setCursor({ block_number: 1n, segment: 0, byte_offset: 0 });
    const block1 = buildBlockPayload(1n, 10n, []);
    const block2 = buildBlockPayload(2n, 20n, []);
    const chunks = [
      { segment: 0, start: 0, bytes: block1, payload_len: block1.length },
      { segment: 1, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
      { segment: 2, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
      { segment: 0, start: 0, bytes: block2, payload_len: block2.length },
      { segment: 1, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
      { segment: 2, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
    ];
    let headCalls = 0;
    let exportCalls = 0;
    const client = {
      getHeadNumber: async (): Promise<bigint> => {
        headCalls += 1;
        if (headCalls === 2) {
          process.emit("SIGINT");
        }
        return 2n;
      },
      exportBlocks: async (
        cursor: { block_number: bigint; segment: number; byte_offset: number } | null
      ): Promise<
        | {
            Ok: {
              chunks: Array<{ segment: number; start: number; bytes: Buffer; payload_len: number }>;
              next_cursor: { block_number: bigint; segment: number; byte_offset: number };
            };
          }
        | { Err: never }
      > => {
        exportCalls += 1;
        if (exportCalls === 1) {
          return {
            Ok: {
              chunks,
              next_cursor: { block_number: 3n, segment: 0, byte_offset: 0 },
            },
          };
        }
        return {
          Ok: {
            chunks: [],
            next_cursor: cursor ?? { block_number: 3n, segment: 0, byte_offset: 0 },
          },
        };
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
      getMetrics: async () => ({
        txs: 0n,
        ema_txs_per_block_x1000: 0n,
        pruned_before_block: null,
        ema_block_rate_per_sec_x1000: 0n,
        total_submitted: 0n,
        window: 128n,
        avg_txs_per_block: 0n,
        block_rate_per_sec_x1000: null,
        cycles: 0n,
        total_dropped: 0n,
        blocks: 0n,
        drop_counts: [],
        queue_len: 0n,
        total_included: 0n,
      }),
    };
    await runWorkerWithDeps(config, db, client, { skipGc: true });
    const cursor = await db.getCursor();
    assert.deepEqual(cursor, { block_number: 3n, segment: 0, byte_offset: 0 });
    const blockCount = await db.queryOne<{ n: string }>("select count(*)::text as n from blocks");
    assert.equal(Number(blockCount?.n ?? "0"), 2);
  });
  await originalClose();
});

test("runWorkerWithDeps recovers from Pruned by rebasing cursor and dropping pending", async () => {
  const db = await createTestIndexerDb();
  const originalClose = db.close.bind(db);
  (db as unknown as { close: () => Promise<void> }).close = async () => {};
  await withTempDir(async (dir) => {
    const config: Config = {
      canisterId: "test-canister",
      icHost: "http://127.0.0.1:4943",
      databaseUrl: "postgres://unused",
      dbPoolMax: 1,
      retentionDays: 90,
      retentionEnabled: false,
      retentionDryRun: false,
      archiveGcDeleteOrphans: false,
      maxBytes: 1_200_000,
      backoffInitialMs: 1,
      backoffMaxMs: 2,
      idlePollMs: 1,
      pruneStatusPollMs: 0,
      opsMetricsPollMs: 0,
      fetchRootKey: false,
      archiveDir: dir,
      chainId: "test",
      zstdLevel: 1,
      maxSegment: 2,
    };
    await db.setCursor({ block_number: 50n, segment: 0, byte_offset: 0 });
    const block101 = buildBlockPayload(101n, 20n, []);
    const cursors: Array<{ block_number: bigint; segment: number; byte_offset: number } | null> = [];
    let headCalls = 0;
    let exportCalls = 0;
    const client = {
      getHeadNumber: async (): Promise<bigint> => {
        headCalls += 1;
        if (headCalls === 5) {
          process.emit("SIGINT");
        }
        return 200n;
      },
      exportBlocks: async (
        cursor: { block_number: bigint; segment: number; byte_offset: number } | null
      ): Promise<
        | {
            Ok: {
              chunks: Array<{ segment: number; start: number; bytes: Buffer; payload_len: number }>;
              next_cursor: { block_number: bigint; segment: number; byte_offset: number };
            };
          }
        | { Err: ExportError }
      > => {
        exportCalls += 1;
        cursors.push(cursor);
        if (exportCalls === 1) {
          return {
            Ok: {
              chunks: [{ segment: 0, start: 0, bytes: Buffer.from([1]), payload_len: 8 }],
              next_cursor: { block_number: 50n, segment: 0, byte_offset: 1 },
            },
          };
        }
        if (exportCalls === 2) {
          return { Err: { Pruned: { pruned_before_block: 100n } } };
        }
        if (exportCalls === 3) {
          return {
            Ok: {
              chunks: [
                { segment: 0, start: 0, bytes: block101, payload_len: block101.length },
                { segment: 1, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
                { segment: 2, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
              ],
              next_cursor: { block_number: 102n, segment: 0, byte_offset: 0 },
            },
          };
        }
        return {
          Ok: {
            chunks: [],
            next_cursor: cursor ?? { block_number: 102n, segment: 0, byte_offset: 0 },
          },
        };
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
      getMetrics: async () => ({
        txs: 0n,
        ema_txs_per_block_x1000: 0n,
        pruned_before_block: null,
        ema_block_rate_per_sec_x1000: 0n,
        total_submitted: 0n,
        window: 128n,
        avg_txs_per_block: 0n,
        block_rate_per_sec_x1000: null,
        cycles: 0n,
        total_dropped: 0n,
        blocks: 0n,
        drop_counts: [],
        queue_len: 0n,
        total_included: 0n,
      }),
    };
    await runWorkerWithDeps(config, db, client, { skipGc: true });
    assert.equal(cursors.length >= 3, true);
    assert.deepEqual(cursors[2], { block_number: 101n, segment: 0, byte_offset: 0 });
    const cursor = await db.getCursor();
    assert.deepEqual(cursor, { block_number: 102n, segment: 0, byte_offset: 0 });
    const block = await db.queryOne<{ n: string }>("select count(*)::text as n from blocks where number = 101");
    assert.equal(Number(block?.n ?? "0"), 1);
    const lastError = await db.queryOne<{ value: string }>("select value from meta where key = $1", ["last_error"]);
    assert.equal(lastError?.value, "Pruned");
    const metricsError = await db.queryOne<{ errors: string }>("select errors::text as errors from metrics_daily");
    assert.equal(Number(metricsError?.errors ?? "0"), 1);
  });
  await originalClose();
});

test("runWorkerWithDeps clamps Pruned cursor to block 1 minimum", async () => {
  const db = await createTestIndexerDb();
  const originalClose = db.close.bind(db);
  (db as unknown as { close: () => Promise<void> }).close = async () => {};
  await withTempDir(async (dir) => {
    const config: Config = {
      canisterId: "test-canister",
      icHost: "http://127.0.0.1:4943",
      databaseUrl: "postgres://unused",
      dbPoolMax: 1,
      retentionDays: 90,
      retentionEnabled: false,
      retentionDryRun: false,
      archiveGcDeleteOrphans: false,
      maxBytes: 1_200_000,
      backoffInitialMs: 1,
      backoffMaxMs: 2,
      idlePollMs: 1,
      pruneStatusPollMs: 0,
      opsMetricsPollMs: 0,
      fetchRootKey: false,
      archiveDir: dir,
      chainId: "test",
      zstdLevel: 1,
      maxSegment: 2,
    };
    await db.setCursor({ block_number: 50n, segment: 0, byte_offset: 0 });
    const cursors: Array<{ block_number: bigint; segment: number; byte_offset: number } | null> = [];
    let headCalls = 0;
    let exportCalls = 0;
    const client = {
      getHeadNumber: async (): Promise<bigint> => {
        headCalls += 1;
        if (headCalls === 3) {
          process.emit("SIGINT");
        }
        return 99n;
      },
      exportBlocks: async (
        cursor: { block_number: bigint; segment: number; byte_offset: number } | null
      ): Promise<
        | {
            Ok: {
              chunks: Array<{ segment: number; start: number; bytes: Buffer; payload_len: number }>;
              next_cursor: { block_number: bigint; segment: number; byte_offset: number };
            };
          }
        | { Err: ExportError }
      > => {
        exportCalls += 1;
        cursors.push(cursor);
        if (exportCalls === 1) {
          return { Err: { Pruned: { pruned_before_block: -2n } } };
        }
        return {
          Ok: {
            chunks: [],
            next_cursor: cursor ?? { block_number: 1n, segment: 0, byte_offset: 0 },
          },
        };
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
      getMetrics: async () => ({
        txs: 0n,
        ema_txs_per_block_x1000: 0n,
        pruned_before_block: null,
        ema_block_rate_per_sec_x1000: 0n,
        total_submitted: 0n,
        window: 128n,
        avg_txs_per_block: 0n,
        block_rate_per_sec_x1000: null,
        cycles: 0n,
        total_dropped: 0n,
        blocks: 0n,
        drop_counts: [],
        queue_len: 0n,
        total_included: 0n,
      }),
    };
    await runWorkerWithDeps(config, db, client, { skipGc: true });
    assert.deepEqual(cursors[1], { block_number: 1n, segment: 0, byte_offset: 0 });
    const persisted = await db.getCursor();
    assert.deepEqual(persisted, { block_number: 1n, segment: 0, byte_offset: 0 });
  });
  await originalClose();
});

test("runWorkerWithDeps clamps Pruned cursor to head when prune floor is ahead", async () => {
  const db = await createTestIndexerDb();
  const originalClose = db.close.bind(db);
  (db as unknown as { close: () => Promise<void> }).close = async () => {};
  await withTempDir(async (dir) => {
    const config: Config = {
      canisterId: "test-canister",
      icHost: "http://127.0.0.1:4943",
      databaseUrl: "postgres://unused",
      dbPoolMax: 1,
      retentionDays: 90,
      retentionEnabled: false,
      retentionDryRun: false,
      archiveGcDeleteOrphans: false,
      maxBytes: 1_200_000,
      backoffInitialMs: 1,
      backoffMaxMs: 2,
      idlePollMs: 1,
      pruneStatusPollMs: 0,
      opsMetricsPollMs: 0,
      fetchRootKey: false,
      archiveDir: dir,
      chainId: "test",
      zstdLevel: 1,
      maxSegment: 2,
    };
    await db.setCursor({ block_number: 50n, segment: 0, byte_offset: 0 });
    const cursors: Array<{ block_number: bigint; segment: number; byte_offset: number } | null> = [];
    let headCalls = 0;
    let exportCalls = 0;
    const client = {
      getHeadNumber: async (): Promise<bigint> => {
        headCalls += 1;
        if (headCalls === 3) {
          process.emit("SIGINT");
        }
        return 77n;
      },
      exportBlocks: async (
        cursor: { block_number: bigint; segment: number; byte_offset: number } | null
      ): Promise<
        | {
            Ok: {
              chunks: Array<{ segment: number; start: number; bytes: Buffer; payload_len: number }>;
              next_cursor: { block_number: bigint; segment: number; byte_offset: number };
            };
          }
        | { Err: ExportError }
      > => {
        exportCalls += 1;
        cursors.push(cursor);
        if (exportCalls === 1) {
          return { Err: { Pruned: { pruned_before_block: 120n } } };
        }
        return {
          Ok: {
            chunks: [],
            next_cursor: cursor ?? { block_number: 77n, segment: 0, byte_offset: 0 },
          },
        };
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
      getMetrics: async () => ({
        txs: 0n,
        ema_txs_per_block_x1000: 0n,
        pruned_before_block: null,
        ema_block_rate_per_sec_x1000: 0n,
        total_submitted: 0n,
        window: 128n,
        avg_txs_per_block: 0n,
        block_rate_per_sec_x1000: null,
        cycles: 0n,
        total_dropped: 0n,
        blocks: 0n,
        drop_counts: [],
        queue_len: 0n,
        total_included: 0n,
      }),
    };
    await runWorkerWithDeps(config, db, client, { skipGc: true });
    assert.deepEqual(cursors[1], { block_number: 77n, segment: 0, byte_offset: 0 });
    const persisted = await db.getCursor();
    assert.deepEqual(persisted, { block_number: 77n, segment: 0, byte_offset: 0 });
  });
  await originalClose();
});

test("runWorkerWithDeps bootstraps MissingData at block 1 instead of head", async () => {
  const db = await createTestIndexerDb();
  const originalClose = db.close.bind(db);
  (db as unknown as { close: () => Promise<void> }).close = async () => {};
  await withTempDir(async (dir) => {
    const config: Config = {
      canisterId: "test-canister",
      icHost: "http://127.0.0.1:4943",
      databaseUrl: "postgres://unused",
      dbPoolMax: 1,
      retentionDays: 90,
      retentionEnabled: false,
      retentionDryRun: false,
      archiveGcDeleteOrphans: false,
      maxBytes: 1_200_000,
      backoffInitialMs: 1,
      backoffMaxMs: 2,
      idlePollMs: 1,
      pruneStatusPollMs: 0,
      opsMetricsPollMs: 0,
      fetchRootKey: false,
      archiveDir: dir,
      chainId: "test",
      zstdLevel: 1,
      maxSegment: 2,
    };
    const cursors: Array<{ block_number: bigint; segment: number; byte_offset: number } | null> = [];
    let headCalls = 0;
    let exportCalls = 0;
    const client = {
      getHeadNumber: async (): Promise<bigint> => {
        headCalls += 1;
        if (headCalls === 3) {
          process.emit("SIGINT");
        }
        return 25n;
      },
      exportBlocks: async (
        cursor: { block_number: bigint; segment: number; byte_offset: number } | null
      ): Promise<
        | {
            Ok: {
              chunks: Array<{ segment: number; start: number; bytes: Buffer; payload_len: number }>;
              next_cursor: { block_number: bigint; segment: number; byte_offset: number };
            };
          }
        | { Err: ExportError }
      > => {
        exportCalls += 1;
        cursors.push(cursor);
        if (exportCalls === 1) {
          return { Err: { MissingData: { message: "missing block 0" } } };
        }
        return {
          Ok: {
            chunks: [],
            next_cursor: cursor ?? { block_number: 1n, segment: 0, byte_offset: 0 },
          },
        };
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
      getMetrics: async () => ({
        txs: 0n,
        ema_txs_per_block_x1000: 0n,
        pruned_before_block: null,
        ema_block_rate_per_sec_x1000: 0n,
        total_submitted: 0n,
        window: 128n,
        avg_txs_per_block: 0n,
        block_rate_per_sec_x1000: null,
        cycles: 0n,
        total_dropped: 0n,
        blocks: 0n,
        drop_counts: [],
        queue_len: 0n,
        total_included: 0n,
      }),
    };
    await runWorkerWithDeps(config, db, client, { skipGc: true });
    assert.deepEqual(cursors[1], { block_number: 1n, segment: 0, byte_offset: 0 });
  });
  await originalClose();
});

test("runWorkerWithDeps exits on final cursor mismatch", async () => {
  const db = await createTestIndexerDb();
  const originalClose = db.close.bind(db);
  (db as unknown as { close: () => Promise<void> }).close = async () => {};
  const originalExit = process.exit;
  try {
    await withTempDir(async (dir) => {
      const config: Config = {
        canisterId: "test-canister",
        icHost: "http://127.0.0.1:4943",
        databaseUrl: "postgres://unused",
        dbPoolMax: 1,
        retentionDays: 90,
        retentionEnabled: false,
        retentionDryRun: false,
        archiveGcDeleteOrphans: false,
        maxBytes: 1_200_000,
        backoffInitialMs: 1,
        backoffMaxMs: 2,
        idlePollMs: 1,
        pruneStatusPollMs: 0,
        opsMetricsPollMs: 0,
        fetchRootKey: false,
        archiveDir: dir,
        chainId: "test",
        zstdLevel: 1,
        maxSegment: 2,
      };
      await db.setCursor({ block_number: 1n, segment: 0, byte_offset: 0 });
      const block1 = buildBlockPayload(1n, 10n, []);
      const chunks = [
        { segment: 0, start: 0, bytes: block1, payload_len: block1.length },
        { segment: 1, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
        { segment: 2, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
      ];
      const client = {
        getHeadNumber: async (): Promise<bigint> => 1n,
        exportBlocks: async (): Promise<
          | {
              Ok: {
                chunks: Array<{ segment: number; start: number; bytes: Buffer; payload_len: number }>;
                next_cursor: { block_number: bigint; segment: number; byte_offset: number };
              };
            }
          | { Err: never }
        > => ({
          Ok: {
            chunks,
            next_cursor: { block_number: 9n, segment: 0, byte_offset: 0 },
          },
        }),
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
        getMetrics: async () => ({
          txs: 0n,
          ema_txs_per_block_x1000: 0n,
          pruned_before_block: null,
          ema_block_rate_per_sec_x1000: 0n,
          total_submitted: 0n,
          window: 128n,
          avg_txs_per_block: 0n,
          block_rate_per_sec_x1000: null,
          cycles: 0n,
          total_dropped: 0n,
          blocks: 0n,
          drop_counts: [],
          queue_len: 0n,
          total_included: 0n,
        }),
      };
      process.exit = ((code?: number) => {
        throw new Error(`EXIT_${code ?? 0}`);
      }) as typeof process.exit;
      await assert.rejects(() => runWorkerWithDeps(config, db, client, { skipGc: true }), /EXIT_1/);
    });
  } finally {
    process.exit = originalExit;
    await originalClose();
  }
});

test("runWorkerWithDeps exits when decoded block number mismatches cursor", async () => {
  const db = await createTestIndexerDb();
  const originalClose = db.close.bind(db);
  (db as unknown as { close: () => Promise<void> }).close = async () => {};
  const originalExit = process.exit;
  try {
    await withTempDir(async (dir) => {
      const config: Config = {
        canisterId: "test-canister",
        icHost: "http://127.0.0.1:4943",
        databaseUrl: "postgres://unused",
        dbPoolMax: 1,
        retentionDays: 90,
        retentionEnabled: false,
        retentionDryRun: false,
        archiveGcDeleteOrphans: false,
        maxBytes: 1_200_000,
        backoffInitialMs: 1,
        backoffMaxMs: 2,
        idlePollMs: 1,
        pruneStatusPollMs: 0,
        opsMetricsPollMs: 0,
        fetchRootKey: false,
        archiveDir: dir,
        chainId: "test",
        zstdLevel: 1,
        maxSegment: 2,
      };
      await db.setCursor({ block_number: 10n, segment: 0, byte_offset: 0 });
      const block12 = buildBlockPayload(12n, 10n, []);
      const chunks = [
        { segment: 0, start: 0, bytes: block12, payload_len: block12.length },
        { segment: 1, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
        { segment: 2, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
      ];
      const client = {
        getHeadNumber: async (): Promise<bigint> => 20n,
        exportBlocks: async (): Promise<
          | {
              Ok: {
                chunks: Array<{ segment: number; start: number; bytes: Buffer; payload_len: number }>;
                next_cursor: { block_number: bigint; segment: number; byte_offset: number };
              };
            }
          | { Err: never }
        > => ({
          Ok: {
            chunks,
            next_cursor: { block_number: 13n, segment: 0, byte_offset: 0 },
          },
        }),
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
        getMetrics: async () => ({
          txs: 0n,
          ema_txs_per_block_x1000: 0n,
          pruned_before_block: null,
          ema_block_rate_per_sec_x1000: 0n,
          total_submitted: 0n,
          window: 128n,
          avg_txs_per_block: 0n,
          block_rate_per_sec_x1000: null,
          cycles: 0n,
          total_dropped: 0n,
          blocks: 0n,
          drop_counts: [],
          queue_len: 0n,
          total_included: 0n,
        }),
      };
      process.exit = ((code?: number) => {
        throw new Error(`EXIT_${code ?? 0}`);
      }) as typeof process.exit;
      await assert.rejects(() => runWorkerWithDeps(config, db, client, { skipGc: true }), /EXIT_1/);
    });
  } finally {
    process.exit = originalExit;
    await originalClose();
  }
});

test("runWorkerWithDeps exits when tx_index and receipts counts differ", async () => {
  const db = await createTestIndexerDb();
  const originalClose = db.close.bind(db);
  (db as unknown as { close: () => Promise<void> }).close = async () => {};
  const originalExit = process.exit;
  try {
    await withTempDir(async (dir) => {
      const config: Config = {
        canisterId: "test-canister",
        icHost: "http://127.0.0.1:4943",
        databaseUrl: "postgres://unused",
        dbPoolMax: 1,
        retentionDays: 90,
        retentionEnabled: false,
        retentionDryRun: false,
        archiveGcDeleteOrphans: false,
        maxBytes: 1_200_000,
        backoffInitialMs: 1,
        backoffMaxMs: 2,
        idlePollMs: 1,
        pruneStatusPollMs: 0,
        opsMetricsPollMs: 0,
        fetchRootKey: false,
        archiveDir: dir,
        chainId: "test",
        zstdLevel: 1,
        maxSegment: 2,
      };
      await db.setCursor({ block_number: 1n, segment: 0, byte_offset: 0 });
      const txHash = Buffer.alloc(32, 0x77);
      const block1 = buildBlockPayload(1n, 10n, [txHash]);
      const txIndexPayload = buildTxIndexPayload(1n, txHash);
      const chunks = [
        { segment: 0, start: 0, bytes: block1, payload_len: block1.length },
        { segment: 1, start: 0, bytes: Buffer.alloc(0), payload_len: 0 },
        { segment: 2, start: 0, bytes: txIndexPayload, payload_len: txIndexPayload.length },
      ];
      const client = {
        getHeadNumber: async (): Promise<bigint> => 1n,
        exportBlocks: async (): Promise<
          | {
              Ok: {
                chunks: Array<{ segment: number; start: number; bytes: Buffer; payload_len: number }>;
                next_cursor: { block_number: bigint; segment: number; byte_offset: number };
              };
            }
          | { Err: never }
        > => ({
          Ok: {
            chunks,
            next_cursor: { block_number: 2n, segment: 0, byte_offset: 0 },
          },
        }),
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
        getMetrics: async () => ({
          txs: 0n,
          ema_txs_per_block_x1000: 0n,
          pruned_before_block: null,
          ema_block_rate_per_sec_x1000: 0n,
          total_submitted: 0n,
          window: 128n,
          avg_txs_per_block: 0n,
          block_rate_per_sec_x1000: null,
          cycles: 0n,
          total_dropped: 0n,
          blocks: 0n,
          drop_counts: [],
          queue_len: 0n,
          total_included: 0n,
        }),
      };
      process.exit = ((code?: number) => {
        throw new Error(`EXIT_${code ?? 0}`);
      }) as typeof process.exit;
      await assert.rejects(() => runWorkerWithDeps(config, db, client, { skipGc: true }), /EXIT_1/);
    });
  } finally {
    process.exit = originalExit;
    await originalClose();
  }
});

test("runWorkerWithDeps exits when stored cursor segment exceeds maxSegment", async () => {
  const db = await createTestIndexerDb();
  const originalClose = db.close.bind(db);
  (db as unknown as { close: () => Promise<void> }).close = async () => {};
  const originalExit = process.exit;
  try {
    await withTempDir(async (dir) => {
      const config: Config = {
        canisterId: "test-canister",
        icHost: "http://127.0.0.1:4943",
        databaseUrl: "postgres://unused",
        dbPoolMax: 1,
        retentionDays: 90,
        retentionEnabled: false,
        retentionDryRun: false,
        archiveGcDeleteOrphans: false,
        maxBytes: 1_200_000,
        backoffInitialMs: 1,
        backoffMaxMs: 2,
        idlePollMs: 1,
        pruneStatusPollMs: 0,
        opsMetricsPollMs: 0,
        fetchRootKey: false,
        archiveDir: dir,
        chainId: "test",
        zstdLevel: 1,
        maxSegment: 2,
      };
      await db.setCursor({ block_number: 1n, segment: 3, byte_offset: 0 });
      const client = {
        getHeadNumber: async (): Promise<bigint> => 1n,
        exportBlocks: async (): Promise<{ Ok: never } | { Err: never }> => {
          throw new Error("exportBlocks should not be called");
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
        getMetrics: async () => ({
          txs: 0n,
          ema_txs_per_block_x1000: 0n,
          pruned_before_block: null,
          ema_block_rate_per_sec_x1000: 0n,
          total_submitted: 0n,
          window: 128n,
          avg_txs_per_block: 0n,
          block_rate_per_sec_x1000: null,
          cycles: 0n,
          total_dropped: 0n,
          blocks: 0n,
          drop_counts: [],
          queue_len: 0n,
          total_included: 0n,
        }),
      };
      process.exit = ((code?: number) => {
        throw new Error(`EXIT_${code ?? 0}`);
      }) as typeof process.exit;
      await assert.rejects(() => runWorkerWithDeps(config, db, client, { skipGc: true }), /EXIT_1/);
    });
  } finally {
    process.exit = originalExit;
    await originalClose();
  }
});

test("runWorkerWithDeps exits when cursor is null and stream cursor is not established", async () => {
  const db = await createTestIndexerDb();
  const originalClose = db.close.bind(db);
  (db as unknown as { close: () => Promise<void> }).close = async () => {};
  const originalExit = process.exit;
  try {
    await withTempDir(async (dir) => {
      const config: Config = {
        canisterId: "test-canister",
        icHost: "http://127.0.0.1:4943",
        databaseUrl: "postgres://unused",
        dbPoolMax: 1,
        retentionDays: 90,
        retentionEnabled: false,
        retentionDryRun: false,
        archiveGcDeleteOrphans: false,
        maxBytes: 1_200_000,
        backoffInitialMs: 1,
        backoffMaxMs: 2,
        idlePollMs: 1,
        pruneStatusPollMs: 0,
        opsMetricsPollMs: 0,
        fetchRootKey: false,
        archiveDir: dir,
        chainId: "test",
        zstdLevel: 1,
        maxSegment: 2,
      };
      const chunkBytes = Buffer.from([1, 2, 3]);
      const client = {
        getHeadNumber: async (): Promise<bigint> => 10n,
        exportBlocks: async (): Promise<
          | {
              Ok: {
                chunks: Array<{ segment: number; start: number; bytes: Buffer; payload_len: number }>;
                next_cursor: { block_number: bigint; segment: number; byte_offset: number };
              };
            }
          | { Err: never }
        > => ({
          Ok: {
            chunks: [{ segment: 0, start: 0, bytes: chunkBytes, payload_len: chunkBytes.length + 5 }],
            next_cursor: { block_number: 1n, segment: 0, byte_offset: chunkBytes.length },
          },
        }),
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
        getMetrics: async () => ({
          txs: 0n,
          ema_txs_per_block_x1000: 0n,
          pruned_before_block: null,
          ema_block_rate_per_sec_x1000: 0n,
          total_submitted: 0n,
          window: 128n,
          avg_txs_per_block: 0n,
          block_rate_per_sec_x1000: null,
          cycles: 0n,
          total_dropped: 0n,
          blocks: 0n,
          drop_counts: [],
          queue_len: 0n,
          total_included: 0n,
        }),
      };
      process.exit = ((code?: number) => {
        throw new Error(`EXIT_${code ?? 0}`);
      }) as typeof process.exit;
      await assert.rejects(() => runWorkerWithDeps(config, db, client, { skipGc: true }), /EXIT_1/);
    });
  } finally {
    process.exit = originalExit;
    await originalClose();
  }
});

test("runWorkerWithDeps does not leak signal listeners after stop", async () => {
  const db = await createTestIndexerDb();
  const originalClose = db.close.bind(db);
  (db as unknown as { close: () => Promise<void> }).close = async () => {};
  const beforeSigInt = process.listenerCount("SIGINT");
  const beforeSigTerm = process.listenerCount("SIGTERM");
  const beforeUncaught = process.listenerCount("uncaughtException");
  const beforeUnhandled = process.listenerCount("unhandledRejection");
  try {
    await withTempDir(async (dir) => {
      const config: Config = {
        canisterId: "test-canister",
        icHost: "http://127.0.0.1:4943",
        databaseUrl: "postgres://unused",
        dbPoolMax: 1,
        retentionDays: 90,
        retentionEnabled: false,
        retentionDryRun: false,
        archiveGcDeleteOrphans: false,
        maxBytes: 1_200_000,
        backoffInitialMs: 1,
        backoffMaxMs: 2,
        idlePollMs: 1,
        pruneStatusPollMs: 0,
        opsMetricsPollMs: 0,
        fetchRootKey: false,
        archiveDir: dir,
        chainId: "test",
        zstdLevel: 1,
        maxSegment: 2,
      };
      let headCalls = 0;
      const client = {
        getHeadNumber: async (): Promise<bigint> => {
          headCalls += 1;
          if (headCalls === 1) {
            process.emit("SIGINT");
          }
          return 1n;
        },
        exportBlocks: async (
          cursor: { block_number: bigint; segment: number; byte_offset: number } | null
        ): Promise<
          | {
              Ok: {
                chunks: Array<{ segment: number; start: number; bytes: Buffer; payload_len: number }>;
                next_cursor: { block_number: bigint; segment: number; byte_offset: number };
              };
            }
          | { Err: never }
        > => ({
          Ok: {
            chunks: [],
            next_cursor: cursor ?? { block_number: 1n, segment: 0, byte_offset: 0 },
          },
        }),
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
        getMetrics: async () => ({
          txs: 0n,
          ema_txs_per_block_x1000: 0n,
          pruned_before_block: null,
          ema_block_rate_per_sec_x1000: 0n,
          total_submitted: 0n,
          window: 128n,
          avg_txs_per_block: 0n,
          block_rate_per_sec_x1000: null,
          cycles: 0n,
          total_dropped: 0n,
          blocks: 0n,
          drop_counts: [],
          queue_len: 0n,
          total_included: 0n,
        }),
      };
      await runWorkerWithDeps(config, db, client, { skipGc: true });
    });
  } finally {
    await originalClose();
  }
  assert.equal(process.listenerCount("SIGINT"), beforeSigInt);
  assert.equal(process.listenerCount("SIGTERM"), beforeSigTerm);
  assert.equal(process.listenerCount("uncaughtException"), beforeUncaught);
  assert.equal(process.listenerCount("unhandledRejection"), beforeUnhandled);
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
    await db.upsertTx({
      tx_hash: Buffer.alloc(32, 2),
      block_number: 10n,
      tx_index: 0,
      caller_principal: null,
      from_address: Buffer.alloc(20, 0x01),
      to_address: Buffer.alloc(20, 0x02),
      receipt_status: 1,
    });
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
    await db.addOpsMetricsSample({
      sampledAtMs: 1_000n,
      queueLen: 2n,
      totalSubmitted: 3n,
      totalIncluded: 1n,
      totalDropped: 1n,
      dropCountsJson: "[]",
      retentionCutoffMs: 900n,
    });
    await db.addOpsMetricsSample({
      sampledAtMs: 2_000n,
      queueLen: 4n,
      totalSubmitted: 7n,
      totalIncluded: 2n,
      totalDropped: 2n,
      dropCountsJson: '[{"code":"1","count":"2"}]',
      retentionCutoffMs: 1_500n,
    });
    const samples = await db.queryOne<{ n: string }>("select count(*)::text as n from ops_metrics_samples");
    assert.equal(Number(samples?.n ?? "0"), 1);
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
    await db.upsertTx({
      tx_hash: Buffer.alloc(32, 11),
      block_number: 1n,
      tx_index: 0,
      caller_principal: null,
      from_address: Buffer.alloc(20, 0x11),
      to_address: Buffer.alloc(20, 0x21),
      receipt_status: 1,
    });
    await db.upsertTx({
      tx_hash: Buffer.alloc(32, 22),
      block_number: 2n,
      tx_index: 0,
      caller_principal: null,
      from_address: Buffer.alloc(20, 0x12),
      to_address: Buffer.alloc(20, 0x22),
      receipt_status: 0,
    });
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
  const db = await IndexerDb.fromPool(pool, { migrations: MIGRATIONS.slice(0, 1) });
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
  const base = 8 + hashLen + hashLen + 8 + 8 + 8 + 8 + hashLen + hashLen + 4;
  const total = base + txIds.length * hashLen;
  const out = Buffer.alloc(total);
  let offset = 0;
  offset = writeU64BE(out, offset, number);
  offset = writeZeros(out, offset, hashLen);
  offset = writeZeros(out, offset, hashLen);
  offset = writeU64BE(out, offset, timestamp);
  offset = writeU64BE(out, offset, 1_000_000_000n); // base_fee_per_gas
  offset = writeU64BE(out, offset, 3_000_000n); // block_gas_limit
  offset = writeU64BE(out, offset, 0n); // gas_used
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

function buildReceiptBytes(status: number, withV2Magic: boolean): Buffer {
  const magic = withV2Magic ? Buffer.from("7263707476320002", "hex") : Buffer.alloc(0);
  const out = Buffer.alloc(magic.length + 32 + 8 + 4 + 1);
  let offset = 0;
  magic.copy(out, offset);
  offset += magic.length;
  offset = writeZeros(out, offset, 32);
  offset = writeU64BE(out, offset, 1n);
  out.writeUInt32BE(0, offset);
  offset += 4;
  out.writeUInt8(status, offset);
  return out;
}

function buildTxIndexPayload(blockNumber: bigint, txHash: Buffer): Buffer {
  const fromAddress = Buffer.alloc(20, 0x11);
  const toAddress = Buffer.alloc(20, 0x22);
  const principalLen = 0;
  const body = Buffer.alloc(12 + 2 + principalLen + fromAddress.length + 1 + toAddress.length);
  body.writeBigUInt64BE(blockNumber, 0);
  body.writeUInt32BE(0, 8);
  body.writeUInt16BE(principalLen, 12);
  fromAddress.copy(body, 14);
  body.writeUInt8(toAddress.length, 14 + fromAddress.length);
  toAddress.copy(body, 14 + fromAddress.length + 1);
  const entryLen = Buffer.alloc(4);
  entryLen.writeUInt32BE(body.length, 0);
  return Buffer.concat([txHash, entryLen, body]);
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
