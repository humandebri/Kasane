// どこで: indexer本体 / 何を: pull→検証→DBコミット / なぜ: 仕様のコミット境界を守るため

import { Config, sleep } from "./config";
import { createClient } from "./client";
import { IndexerDb } from "./db";
import { decodeBlockPayload, decodeTxIndexPayload } from "./decode";
import { Chunk, Cursor, ExportError, ExportResponse, Result } from "./types";

export async function runWorker(config: Config): Promise<void> {
  const db = new IndexerDb(config.dbPath);
  const client = await createClient(config);
  let cursor: Cursor | null = db.getCursor();
  let backoffMs = config.backoffInitialMs;
  let pending: Pending | null = null;

  for (;;) {
    let headNumber: bigint;
    try {
      headNumber = await client.getHeadNumber();
    } catch (err) {
      logError("head fetch failed", err);
      await sleep(backoffMs);
      backoffMs = nextBackoff(backoffMs, config.backoffMaxMs);
      continue;
    }

    let result: Result<ExportResponse, ExportError>;
    try {
      result = await client.exportBlocks(cursor, config.maxBytes);
    } catch (err) {
      logError("export_blocks network error", err);
      await sleep(backoffMs);
      backoffMs = nextBackoff(backoffMs, config.backoffMaxMs);
      continue;
    }

    if ("Err" in result) {
      handleExportError(result.Err, db);
      break;
    }

    const response = result.Ok;
    if (response.chunks.length === 0) {
      await sleep(backoffMs);
      backoffMs = nextBackoff(backoffMs, config.backoffMaxMs);
      continue;
    }
    backoffMs = config.backoffInitialMs;

    if (!response.next_cursor) {
      throw new Error("export_blocks returned no next_cursor");
    }

    if (cursor) {
      enforceNextCursor(response, cursor);
    }

    if (!pending) {
      if (cursor) {
        pending = newPending(cursor);
      } else {
        pending = newPendingFromChunk(response.chunks[0]);
      }
    }

    applyChunks(pending, response.chunks, cursor);
    const previousCursor = cursor;
    cursor = response.next_cursor;

    if (pending.complete) {
      const blockInfo = decodeBlockPayload(pending.payloads[0]);
      const txIndex = decodeTxIndexPayload(pending.payloads[2]);
      const rawBytes = sumChunkBytes(response.chunks);
      const blocksIngested = previousCursor && cursor
        ? cursor.block_number === previousCursor.block_number + 1n
          ? 1
          : 0
        : 0;
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
        if (!cursor) {
          throw new Error("cursor missing on commit");
        }
        db.setCursor(cursor);
        db.addMetrics(toDayKey(), rawBytes, blocksIngested, 0);
        db.setMeta("last_head", headNumber.toString());
        db.setMeta("last_ingest_at", Date.now().toString());
      });
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

function handleExportError(error: ExportError, db: IndexerDb): void {
  if ("Pruned" in error) {
    db.setMeta("last_error", "Pruned");
    db.addMetrics(toDayKey(), 0, 0, 1);
    throw new Error(`Pruned before ${error.Pruned.pruned_before_block.toString()}`);
  }
  if ("InvalidCursor" in error) {
    db.setMeta("last_error", "InvalidCursor");
    db.addMetrics(toDayKey(), 0, 0, 1);
    throw new Error(`InvalidCursor: ${error.InvalidCursor.message}`);
  }
  if ("MissingData" in error) {
    db.setMeta("last_error", "MissingData");
    db.addMetrics(toDayKey(), 0, 0, 1);
    throw new Error(`MissingData: ${error.MissingData.message}`);
  }
  db.setMeta("last_error", "Limit");
  db.addMetrics(toDayKey(), 0, 0, 1);
  throw new Error("Limit: max_bytes invalid");
}

function nextBackoff(current: number, max: number): number {
  const next = current * 2;
  return next > max ? max : next;
}

function logError(message: string, err: unknown): void {
  const detail = err instanceof Error ? err.message : String(err);
  process.stderr.write(`[indexer] ${message}: ${detail}\n`);
}

function sumChunkBytes(chunks: Chunk[]): number {
  let total = 0;
  for (const chunk of chunks) {
    total += chunk.bytes.length;
  }
  return total;
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
