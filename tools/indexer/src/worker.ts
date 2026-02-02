// どこで: indexer本体 / 何を: pull→検証→DBコミット / なぜ: 仕様のコミット境界を守るため

import { Config, sleep } from "./config";
import { createClient } from "./client";
import { IndexerDb } from "./db";
import { archiveBlock } from "./archiver";
import { runArchiveGc } from "./archive_gc";
import { decodeBlockPayload, decodeTxIndexPayload } from "./decode";
import { Chunk, Cursor, ExportError, ExportResponse, Result } from "./types";

export async function runWorker(config: Config): Promise<void> {
  const db = new IndexerDb(config.dbPath);
  try {
    await runArchiveGc(db, config.archiveDir, config.chainId);
  } catch (err) {
    logError("archive gc failed", err);
  }
  const client = await createClient(config);
  let cursor: Cursor | null = db.getCursor();
  let backoffMs = config.backoffInitialMs;
  let pending: Pending | null = null;
  let retryCount = 0;
  let lastBackoffLogAt = 0;
  let stopRequested = false;

  setupSignalHandlers(() => {
    stopRequested = true;
  });

  for (;;) {
    if (stopRequested) {
      process.stderr.write("[indexer] stop requested; exiting loop\n");
      break;
    }
    let headNumber: bigint;
    try {
      headNumber = await client.getHeadNumber();
    } catch (err) {
      logError("head fetch failed", err);
      retryCount += 1;
      logRetry(backoffMs, retryCount, err);
      await sleep(backoffMs);
      backoffMs = nextBackoff(backoffMs, config.backoffMaxMs);
      continue;
    }

    let result: Result<ExportResponse, ExportError>;
    try {
      result = await client.exportBlocks(cursor, config.maxBytes);
    } catch (err) {
      logError("export_blocks network error", err);
      retryCount += 1;
      logRetry(backoffMs, retryCount, err);
      await sleep(backoffMs);
      backoffMs = nextBackoff(backoffMs, config.backoffMaxMs);
      continue;
    }

    if ("Err" in result) {
      const classified = classifyExportError(result.Err, db);
      logFatal(classified.kind, cursor, headNumber, null, null, classified.message);
      process.exit(1);
    }

    const response = result.Ok;
    if (response.chunks.length === 0) {
      retryCount = 0;
      const nextBackoffMs = nextBackoff(backoffMs, config.backoffMaxMs);
      logBackoffOnce(backoffMs, nextBackoffMs, lastBackoffLogAt, (ts) => {
        lastBackoffLogAt = ts;
      });
      await sleep(nextBackoffMs);
      backoffMs = nextBackoffMs;
      continue;
    }
    backoffMs = config.backoffInitialMs;
    retryCount = 0;
    lastBackoffLogAt = 0;

    if (!response.next_cursor) {
      logFatal("InvalidCursor", cursor, headNumber, response, null, "missing next_cursor");
      process.exit(1);
    }

    if (cursor) {
      try {
        enforceNextCursor(response, cursor);
      } catch (err) {
        logFatal("InvalidCursor", cursor, headNumber, response, null, errMessage(err));
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
      logFatal("InvalidCursor", cursor, headNumber, response, null, errMessage(err));
      process.exit(1);
    }
    const previousCursor = cursor;
    cursor = response.next_cursor;

    if (pending.complete) {
      let blockInfo;
      let txIndex;
      try {
        blockInfo = decodeBlockPayload(pending.payloads[0]);
        txIndex = decodeTxIndexPayload(pending.payloads[2]);
      } catch (err) {
        logFatal("Decode", previousCursor, headNumber, response, null, errMessage(err));
        process.exit(1);
      }
      let archive;
      try {
        archive = await archiveBlock({
          archiveDir: config.archiveDir,
          chainId: config.chainId,
          blockNumber: blockInfo.number,
          blockPayload: pending.payloads[0],
          receiptsPayload: pending.payloads[1],
          txIndexPayload: pending.payloads[2],
          zstdLevel: config.zstdLevel,
        });
      } catch (err) {
        logFatal("ArchiveIO", previousCursor, headNumber, response, null, errMessage(err));
        process.exit(1);
      }
      const blocksIngested =
        previousCursor && cursor && cursor.block_number === previousCursor.block_number + 1n ? 1 : 0;
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
        logFatal("ArchiveIO", previousCursor, headNumber, response, archive, errMessage(err));
        process.exit(1);
      }
      pending = null;
    }
  }

  db.close();
}

type Pending = {
  payloads: Buffer[];
  payloadLens: number[];
  segment: number;
  offset: number;
  complete: boolean;
};

function newPending(cursor: Cursor): Pending {
  return {
    payloads: [Buffer.alloc(0), Buffer.alloc(0), Buffer.alloc(0)],
    payloadLens: [0, 0, 0],
    segment: cursor.segment,
    offset: cursor.byte_offset,
    complete: false,
  };
}

function newPendingFromChunk(chunk: Chunk): Pending {
  return {
    payloads: [Buffer.alloc(0), Buffer.alloc(0), Buffer.alloc(0)],
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
      pending.payloads[segIndex] = Buffer.concat([pending.payloads[segIndex], Buffer.from(chunk.bytes)]);
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

function logError(message: string, err: unknown): void {
  const detail = err instanceof Error ? err.message : String(err);
  process.stderr.write(`[indexer] ${message}: ${detail}\n`);
}

function logRetry(backoffMs: number, retryCount: number, err: unknown): void {
  const detail = errMessage(err);
  const payload = {
    level: "warn",
    event: "retry",
    retry_count: retryCount,
    backoff_ms: backoffMs,
    error: detail,
    ts: Date.now(),
  };
  process.stderr.write(`${JSON.stringify(payload)}\n`);
}

function logBackoffOnce(previous: number, next: number, lastLoggedAt: number, update: (ts: number) => void): void {
  if (next <= previous) {
    return;
  }
  const now = Date.now();
  if (now - lastLoggedAt < 60_000) {
    return;
  }
  update(now);
  const payload = {
    level: "info",
    event: "idle_backoff",
    backoff_ms: next,
    ts: now,
  };
  process.stderr.write(`${JSON.stringify(payload)}\n`);
}

function logFatal(
  kind: "Pruned" | "InvalidCursor" | "Decode" | "ArchiveIO",
  cursor: Cursor | null,
  head: bigint,
  response: ExportResponse | null,
  archive: { path: string; sizeBytes: number; sha256: Buffer } | null,
  message: string
): void {
  const summary = response ? summarizeChunks(response.chunks) : null;
  const payload = {
    level: "error",
    event: "fatal",
    error_kind: kind,
    cursor: cursor ? cursorToJsonSafe(cursor) : null,
    head: head.toString(),
    next_cursor: response?.next_cursor ? cursorToJsonSafe(response.next_cursor) : null,
    chunks_summary: summary,
    archive: archive
      ? {
          path: archive.path,
          size_bytes: archive.sizeBytes,
          sha256: archive.sha256.toString("hex"),
        }
      : null,
    message,
    ts: Date.now(),
  };
  process.stderr.write(`${JSON.stringify(payload)}\n`);
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

function setupSignalHandlers(onStop: () => void): void {
  const handler = (signal: NodeJS.Signals) => {
    process.stderr.write(`[indexer] received ${signal}, stopping after current loop\n`);
    onStop();
  };
  process.on("SIGINT", handler);
  process.on("SIGTERM", handler);
}

function toDayKey(): number {
  const now = new Date();
  const year = now.getUTCFullYear();
  const month = String(now.getUTCMonth() + 1).padStart(2, "0");
  const day = String(now.getUTCDate()).padStart(2, "0");
  return Number(`${year}${month}${day}`);
}

function enforceNextCursor(response: ExportResponse, cursor: Cursor): void {
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
