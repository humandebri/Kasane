/// <reference path="./globals.d.ts" />
// どこで: indexerのpending集計 / 何を: chunk検証とpayload再構築 / なぜ: セグメント境界の安全性を担保するため

import { Chunk, Cursor, ExportResponse } from "./types";

export type Pending = {
  payloadParts: Buffer[][];
  payloadLens: number[];
  segment: number;
  offset: number;
  complete: boolean;
};

export function newPending(cursor: Cursor): Pending {
  return {
    payloadParts: [[], [], []],
    payloadLens: [0, 0, 0],
    segment: cursor.segment,
    offset: cursor.byte_offset,
    complete: false,
  };
}

export function newPendingFromChunk(chunk: Chunk): Pending {
  return {
    payloadParts: [[], [], []],
    payloadLens: [0, 0, 0],
    segment: chunk.segment,
    offset: chunk.start,
    complete: false,
  };
}

export function applyChunks(pending: Pending, chunks: Chunk[], cursor: Cursor | null): void {
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

export function finalizePayloads(pending: Pending): Buffer[] {
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

export function totalChunkBytes(chunks: Chunk[]): number {
  let total = 0;
  for (const chunk of chunks) {
    total += chunk.bytes.length;
  }
  return total;
}

export function enforceNextCursor(response: ExportResponse, cursor: Cursor): void {
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
