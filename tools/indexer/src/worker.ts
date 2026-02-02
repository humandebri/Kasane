/// <reference path="./globals.d.ts" />
// どこで: indexer本体 / 何を: pull→検証→DBコミット / なぜ: 仕様のコミット境界を守るため

import { Config, sleep } from "./config";
import { createClient } from "./client";
import { IndexerDb } from "./db";
import { archiveBlock } from "./archiver";
import { runArchiveGc } from "./archive_gc";
import { decodeBlockPayload, decodeTxIndexPayload } from "./decode";
import { Chunk, Cursor, ExportError, ExportResponse, Result } from "./types";
import type { ArchiveResult } from "./archiver";
import type { BlockInfo, TxIndexInfo } from "./decode";

export async function runWorker(config: Config): Promise<void> {
  const db = new IndexerDb(config.dbPath);
  const client = await createClient(config);
  await runWorkerWithDeps(config, db, client, { skipGc: false });
}

export async function runWorkerWithDeps(
  config: Config,
  db: IndexerDb,
  client: {
    getHeadNumber: () => Promise<bigint>;
    exportBlocks: (cursor: Cursor | null, maxBytes: number) => Promise<Result<ExportResponse, ExportError>>;
  },
  options: { skipGc: boolean }
): Promise<void> {
  if (!options.skipGc) {
    try {
      await runArchiveGc(db, config.archiveDir, config.chainId);
    } catch (err) {
      logWarn(config.chainId, "archive_gc_failed", { message: errMessage(err) });
    }
  }
  let cursor: Cursor | null = db.getCursor();
  let lastHead: bigint | null = null;
  let backoffMs = config.backoffInitialMs;
  let pending: Pending | null = null;
  let retryCount = 0;
  let lastIdleLogAt = 0;
  let stopRequested = false;

  setupSignalHandlers(config.chainId, () => {
    stopRequested = true;
  });
  setupFatalHandlers((err) => {
    logFatal(config.chainId, "Unknown", cursor, lastHead, null, null, "uncaught", err);
    process.exit(1);
  });

  for (;;) {
    if (stopRequested) {
      logInfo(config.chainId, "stop_requested", { message: "exiting loop" });
      break;
    }
    let headNumber: bigint;
    try {
      headNumber = await client.getHeadNumber();
    } catch (err) {
      retryCount += 1;
      logRetry(config.chainId, backoffMs, retryCount, "head_fetch_failed", err);
      await sleep(backoffMs);
      backoffMs = nextBackoff(backoffMs, config.backoffMaxMs);
      continue;
    }
    lastHead = headNumber;

    let result: Result<ExportResponse, ExportError>;
    try {
      result = await client.exportBlocks(cursor, config.maxBytes);
    } catch (err) {
      retryCount += 1;
      logRetry(config.chainId, backoffMs, retryCount, "export_blocks_failed", err);
      await sleep(backoffMs);
      backoffMs = nextBackoff(backoffMs, config.backoffMaxMs);
      continue;
    }

    if ("Err" in result) {
      const classified = classifyExportError(result.Err, db);
      logFatal(
        config.chainId,
        classified.kind,
        cursor,
        headNumber,
        null,
        null,
        classified.message
      );
      process.exit(1);
    } else {
      const response = result.Ok;
      if (response.chunks.length === 0) {
        retryCount = 0;
        const idleSleep = config.idlePollMs;
        logIdle(config.chainId, cursor, headNumber, idleSleep, lastIdleLogAt, (ts) => {
          lastIdleLogAt = ts;
        });
        await sleep(idleSleep);
        backoffMs = config.backoffInitialMs;
        continue;
      }
      backoffMs = config.backoffInitialMs;
      retryCount = 0;
      lastIdleLogAt = 0;
      if (totalChunkBytes(response.chunks) > config.maxBytes) {
        logFatal(
          config.chainId,
          "InvalidCursor",
          cursor,
          headNumber,
          response,
          null,
          "chunk bytes exceed max_bytes"
        );
        process.exit(1);
      }

    if (!response.next_cursor) {
      logFatal(
        config.chainId,
        "InvalidCursor",
        cursor,
        headNumber,
        response,
        null,
        "missing next_cursor"
      );
      process.exit(1);
    }

    if (cursor) {
      try {
        enforceNextCursor(response, cursor);
      } catch (err) {
        logFatal(
          config.chainId,
          "InvalidCursor",
          cursor,
          headNumber,
          response,
          null,
          errMessage(err),
          err
        );
        process.exit(1);
      }
    }

    if (!pending) {
      if (cursor) {
        pending = newPending(cursor);
      } else {
        pending = newPendingFromChunk(response.chunks[0]);
      }
    }

    try {
      applyChunks(pending, response.chunks, cursor);
    } catch (err) {
      logFatal(
        config.chainId,
        "InvalidCursor",
        cursor,
        headNumber,
        response,
        null,
        errMessage(err),
        err
      );
      process.exit(1);
    }
    const previousCursor = cursor;
    cursor = response.next_cursor;

    if (pending.complete) {
      let blockInfo: BlockInfo | null = null;
      let txIndex: TxIndexInfo[] | null = null;
      const payloads = finalizePayloads(pending);
      try {
        blockInfo = decodeBlockPayload(payloads[0]);
        txIndex = decodeTxIndexPayload(payloads[2]);
      } catch (err) {
        logFatal(
          config.chainId,
          "Decode",
          previousCursor,
          headNumber,
          response,
          null,
          errMessage(err),
          err
        );
        process.exit(1);
      }
      if (!blockInfo || !txIndex) {
        logFatal(
          config.chainId,
          "Decode",
          previousCursor,
          headNumber,
          response,
          null,
          "decode result missing"
        );
        process.exit(1);
      }
      let archive: ArchiveResult | null = null;
      try {
        archive = await archiveBlock({
          archiveDir: config.archiveDir,
          chainId: config.chainId,
          blockNumber: blockInfo.number,
          blockPayload: payloads[0],
          receiptsPayload: payloads[1],
          txIndexPayload: payloads[2],
          zstdLevel: config.zstdLevel,
        });
      } catch (err) {
        logFatal(
          config.chainId,
          "ArchiveIO",
          previousCursor,
          headNumber,
          response,
          null,
          errMessage(err),
          err
        );
        process.exit(1);
      }
      if (!archive) {
        logFatal(
          config.chainId,
          "ArchiveIO",
          previousCursor,
          headNumber,
          response,
          null,
          "archive result missing"
        );
        process.exit(1);
      }
      const blocksIngested = 1;
      try {
        db.transaction(() => {
          db.upsertBlock({
            number: blockInfo.number,
            hash: blockInfo.blockHash,
            timestamp: blockInfo.timestamp,
            tx_count: blockInfo.txIds.length,
          });
          for (const entry of txIndex) {
            db.upsertTx({
              tx_hash: entry.txHash,
              block_number: entry.blockNumber,
              tx_index: entry.txIndex,
            });
          }
          db.addArchive({
            blockNumber: blockInfo.number,
            path: archive.path,
            sha256: archive.sha256,
            sizeBytes: archive.sizeBytes,
            rawBytes: archive.rawBytes,
            createdAt: Date.now(),
          });
          if (!cursor) {
            throw new Error("cursor missing on commit");
          }
          db.setCursor(cursor);
          db.addMetrics(toDayKey(), archive.rawBytes, archive.sizeBytes, blocksIngested, 0);
          db.setMeta("last_head", headNumber.toString());
          db.setMeta("last_ingest_at", Date.now().toString());
        });
      } catch (err) {
        logFatal(
          config.chainId,
          "Db",
          previousCursor,
          headNumber,
          response,
          archive,
          errMessage(err),
          err
        );
        process.exit(1);
      }
      pending = null;
    }
    }
  }

  db.close();
}

type Pending = {
  payloadParts: Buffer[][];
  payloadLens: number[];
  segment: number;
  offset: number;
  complete: boolean;
};

function newPending(cursor: Cursor): Pending {
  return {
    payloadParts: [[], [], []],
    payloadLens: [0, 0, 0],
    segment: cursor.segment,
    offset: cursor.byte_offset,
    complete: false,
  };
}

function newPendingFromChunk(chunk: Chunk): Pending {
  return {
    payloadParts: [[], [], []],
    payloadLens: [0, 0, 0],
    segment: chunk.segment,
    offset: chunk.start,
    complete: false,
  };
}

function applyChunks(pending: Pending, chunks: Chunk[], cursor: Cursor | null): void {
  if (chunks.length === 0) {
    throw new Error("applyChunks called with empty chunks");
  }
  const first = chunks[0];
  if (cursor) {
    if (first.segment !== cursor.segment || first.start !== cursor.byte_offset) {
      throw new Error("cursor and first chunk mismatch");
    }
  } else {
    if (first.segment !== pending.segment || first.start !== pending.offset) {
      throw new Error("initial chunk mismatch without cursor");
    }
  }
  for (const chunk of chunks) {
    // 連続性と単調増加を保証し、ギャップを許さない
    if (chunk.segment !== pending.segment) {
      throw new Error("chunk segment is out of order");
    }
    if (chunk.start !== pending.offset) {
      throw new Error("chunk start mismatch");
    }
    const segIndex = pending.segment;
    const payloadLen = ensurePayloadLen(pending, segIndex, chunk.payload_len);
    if (chunk.start > payloadLen) {
      throw new Error("chunk start out of range");
    }
    if (chunk.bytes.length === 0) {
      // empty chunkは「このセグメントを完走済み」の合図のみ許可
      if (pending.offset !== payloadLen) {
        throw new Error("empty chunk before segment end");
      }
    } else {
      const nextOffset = pending.offset + chunk.bytes.length;
      if (nextOffset > payloadLen) {
        throw new Error("chunk exceeds payload length");
      }
      pending.payloadParts[segIndex].push(Buffer.from(chunk.bytes));
      pending.offset = nextOffset;
    }

    // セグメント完走時のみ次へ進む
    if (pending.offset === payloadLen) {
      if (pending.segment === 2) {
        pending.complete = true;
        return;
      }
      pending.segment += 1;
      pending.offset = 0;
    }
  }
}

function ensurePayloadLen(pending: Pending, segment: number, len: number): number {
  const existing = pending.payloadLens[segment];
  if (existing === 0) {
    pending.payloadLens[segment] = len;
    return len;
  }
  if (existing !== len) {
    throw new Error("payload_len mismatch");
  }
  return existing;
}

function finalizePayloads(pending: Pending): Buffer[] {
  const out: Buffer[] = [];
  for (let i = 0; i < 3; i += 1) {
    const len = pending.payloadLens[i] ?? 0;
    const parts = pending.payloadParts[i] ?? [];
    if (len === 0) {
      out.push(Buffer.alloc(0));
      continue;
    }
    if (parts.length === 0) {
      throw new Error("missing payload parts");
    }
    out.push(Buffer.concat(parts, len));
  }
  return out;
}

function totalChunkBytes(chunks: Chunk[]): number {
  let total = 0;
  for (const chunk of chunks) {
    total += chunk.bytes.length;
  }
  return total;
}

function classifyExportError(
  error: ExportError,
  db: IndexerDb
): { kind: "Pruned" | "InvalidCursor" | "Decode" | "ArchiveIO"; message: string } {
  if ("Pruned" in error) {
    db.setMeta("last_error", "Pruned");
    db.addMetrics(toDayKey(), 0, 0, 0, 1);
    return { kind: "Pruned", message: `Pruned before ${error.Pruned.pruned_before_block.toString()}` };
  }
  if ("InvalidCursor" in error) {
    db.setMeta("last_error", "InvalidCursor");
    db.addMetrics(toDayKey(), 0, 0, 0, 1);
    return { kind: "InvalidCursor", message: `InvalidCursor: ${error.InvalidCursor.message}` };
  }
  if ("MissingData" in error) {
    db.setMeta("last_error", "MissingData");
    db.addMetrics(toDayKey(), 0, 0, 0, 1);
    return { kind: "Decode", message: `MissingData: ${error.MissingData.message}` };
  }
  db.setMeta("last_error", "Limit");
  db.addMetrics(toDayKey(), 0, 0, 0, 1);
  return { kind: "InvalidCursor", message: "Limit: max_bytes invalid" };
}

function nextBackoff(current: number, max: number): number {
  const next = current * 2;
  return next > max ? max : next;
}

function logInfo(chainId: string, event: string, payload: Record<string, unknown>): void {
  logJson("info", chainId, event, payload);
}

function logWarn(chainId: string, event: string, payload: Record<string, unknown>): void {
  logJson("warn", chainId, event, payload);
}

function logError(chainId: string, event: string, payload: Record<string, unknown>): void {
  logJson("error", chainId, event, payload);
}

function logJson(
  level: "info" | "warn" | "error",
  chainId: string,
  event: string,
  payload: Record<string, unknown>
): void {
  const out = {
    ts_ms: Date.now(),
    level,
    event,
    chain_id: chainId,
    pid: process.pid,
    ...payload,
  };
  process.stderr.write(`${JSON.stringify(out)}\n`);
}

function logRetry(
  chainId: string,
  backoffMs: number,
  retryCount: number,
  reason: string,
  err: unknown
): void {
  logWarn(chainId, "retry", {
    retry_count: retryCount,
    backoff_ms: backoffMs,
    reason,
    err: errMessage(err),
  });
}

function logIdle(
  chainId: string,
  cursor: Cursor | null,
  head: bigint,
  sleepMs: number,
  lastLoggedAt: number,
  update: (ts: number) => void
): void {
  const now = Date.now();
  if (now - lastLoggedAt < 60_000) {
    return;
  }
  update(now);
  logInfo(chainId, "idle", {
    cursor: cursor ? cursorToJsonSafe(cursor) : null,
    head: head.toString(),
    cursor_lag: cursor ? toLag(head, cursor) : null,
    sleep_ms: sleepMs,
  });
}

function logFatal(
  chainId: string,
  kind: "Pruned" | "InvalidCursor" | "Decode" | "ArchiveIO" | "Db" | "Net" | "Unknown",
  cursor: Cursor | null,
  head: bigint | null,
  response: ExportResponse | null,
  archive: { path: string; sizeBytes: number; sha256: Buffer } | null,
  message: string,
  err?: unknown
): void {
  const summary = response ? summarizeChunks(response.chunks) : null;
  logError(chainId, "fatal", {
    error_kind: kind,
    cursor: cursor ? cursorToJsonSafe(cursor) : null,
    head: head ? head.toString() : null,
    next_cursor: response?.next_cursor ? cursorToJsonSafe(response.next_cursor) : null,
    cursor_lag: head && cursor ? toLag(head, cursor) : null,
    chunks_summary: summary,
    archive: archive
      ? {
          path: archive.path,
          size_bytes: archive.sizeBytes,
          sha256_hex: archive.sha256.toString("hex"),
        }
      : null,
    message,
    err: errToJson(err),
  });
}

function summarizeChunks(chunks: Chunk[]): {
  count: number;
  total_bytes: number;
  first: ChunkSummary | null;
  last: ChunkSummary | null;
} {
  let total = 0;
  for (const chunk of chunks) {
    total += chunk.bytes.length;
  }
  const first = chunks.length > 0 ? toSummary(chunks[0]) : null;
  const last = chunks.length > 0 ? toSummary(chunks[chunks.length - 1]) : null;
  return {
    count: chunks.length,
    total_bytes: total,
    first,
    last,
  };
}

type ChunkSummary = {
  segment: number;
  start: number;
  payload_len: number;
};

function toSummary(chunk: Chunk): ChunkSummary {
  return {
    segment: chunk.segment,
    start: chunk.start,
    payload_len: chunk.payload_len,
  };
}

function cursorToJsonSafe(cursor: Cursor): { block_number: string; segment: number; byte_offset: number; v: number } {
  return {
    v: 1,
    block_number: cursor.block_number.toString(),
    segment: cursor.segment,
    byte_offset: cursor.byte_offset,
  };
}

function errMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function errToJson(err: unknown): { name?: string; message?: string; stack?: string } {
  if (!err) {
    return {};
  }
  if (err instanceof Error) {
    return {
      name: err.name,
      message: err.message,
      stack: err.stack,
    };
  }
  return { message: String(err) };
}

function toLag(head: bigint, cursor: Cursor): number {
  const diff = head - cursor.block_number;
  if (diff < 0n) {
    return 0;
  }
  const limit = BigInt(Number.MAX_SAFE_INTEGER);
  if (diff > limit) {
    return Number.MAX_SAFE_INTEGER;
  }
  return Number(diff);
}

function setupSignalHandlers(chainId: string, onStop: () => void): void {
  const handler = (signal: NodeJS.Signals) => {
    logInfo(chainId, "signal", { signal });
    onStop();
  };
  process.on("SIGINT", handler);
  process.on("SIGTERM", handler);
}

function setupFatalHandlers(onFatal: (err: unknown) => void): void {
  process.on("uncaughtException", (err) => {
    onFatal(err);
  });
  process.on("unhandledRejection", (err) => {
    onFatal(err);
  });
}

function toDayKey(): number {
  const now = new Date();
  const year = now.getUTCFullYear();
  const month = String(now.getUTCMonth() + 1).padStart(2, "0");
  const day = String(now.getUTCDate()).padStart(2, "0");
  return Number(`${year}${month}${day}`);
}

function enforceNextCursor(response: ExportResponse, cursor: Cursor): void {
  if (!response.next_cursor) {
    throw new Error("missing next_cursor");
  }
  if (response.chunks.length === 0) {
    if (response.next_cursor.block_number !== cursor.block_number) {
      throw new Error("next_cursor block_number mismatch on empty response");
    }
    return;
  }
  const first = response.chunks[0];
  if (first.segment !== cursor.segment || first.start !== cursor.byte_offset) {
    throw new Error("cursor and first chunk mismatch");
  }
  const nextBlock = response.next_cursor.block_number;
  if (nextBlock !== cursor.block_number && nextBlock !== cursor.block_number + 1n) {
    throw new Error("next_cursor block_number out of range");
  }
}

export const _test = {
  applyChunks,
  enforceNextCursor,
  newPending,
  newPendingFromChunk,
  totalChunkBytes,
};
