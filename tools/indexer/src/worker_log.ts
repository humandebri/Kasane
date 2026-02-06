/// <reference path="./globals.d.ts" />
// どこで: indexerログ / 何を: JSONログとエラー整形 / なぜ: 監視・解析を安定させるため

import { Chunk, Cursor, ExportResponse } from "./types";

export function logInfo(chainId: string, event: string, payload: Record<string, unknown>): void {
  logJson("info", chainId, event, payload);
}

export function logWarn(chainId: string, event: string, payload: Record<string, unknown>): void {
  logJson("warn", chainId, event, payload);
}

export function logRetry(
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

export function logIdle(
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

export function logFatal(
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

export function errMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
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
