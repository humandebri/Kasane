// どこで: cursorの永続化 / 何を: JSONエンコード/デコード / なぜ: 互換性と可読性を固定するため

import { Cursor } from "./types";

export function cursorToJson(cursor: Cursor): string {
  const payload = {
    v: 1,
    block_number: cursor.block_number.toString(),
    segment: cursor.segment,
    byte_offset: cursor.byte_offset,
  };
  return JSON.stringify(payload);
}

export function cursorFromJson(text: string): Cursor {
  const parsed: unknown = JSON.parse(text);
  if (!isRecord(parsed)) {
    throw new Error("cursor JSON must be an object");
  }
  const versionValue = parsed.v;
  const blockValue = parsed.block_number;
  const segmentValue = parsed.segment;
  const offsetValue = parsed.byte_offset;

  if (versionValue !== 1 && versionValue !== undefined) {
    throw new Error("cursor.v must be 1");
  }
  const blockNumber = parseBigInt(blockValue, "block_number", () => {
    warnDeprecated("block_number as number is deprecated; use string");
  });
  const segment = parseNumber(segmentValue, "segment");
  const byteOffset = parseNumber(offsetValue, "byte_offset");
  if (segment < 0 || segment > 2) {
    throw new Error("cursor.segment out of range");
  }
  if (byteOffset < 0) {
    throw new Error("cursor.byte_offset out of range");
  }
  return {
    block_number: blockNumber,
    segment,
    byte_offset: byteOffset,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function parseNumber(value: unknown, name: string): number {
  if (typeof value === "number" && Number.isFinite(value)) {
    return Math.floor(value);
  }
  if (typeof value === "string") {
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) {
      throw new Error(`${name} must be a number`);
    }
    return Math.floor(parsed);
  }
  throw new Error(`${name} must be a number`);
}

function parseBigInt(value: unknown, name: string, onNumber: () => void): bigint {
  if (typeof value === "number" && Number.isFinite(value)) {
    if (!Number.isSafeInteger(value)) {
      throw new Error(`${name} exceeds safe integer range`);
    }
    onNumber();
    return BigInt(value);
  }
  if (typeof value === "string") {
    if (!/^(0|[1-9][0-9]*)$/.test(value)) {
      throw new Error(`${name} must be a base-10 string`);
    }
    return BigInt(value);
  }
  throw new Error(`${name} must be a string or number`);
}

function warnDeprecated(message: string): void {
  process.stderr.write(`[indexer] deprecated cursor format: ${message}\n`);
}
