// どこで: ローカルアーカイブ / 何を: 1ブロック分の4セグメントbundleをzstd圧縮保存 / なぜ: prune前の保険を現行内容と一致した形で保持するため

import { createHash } from "crypto";
import { promises as fs } from "node:fs";
import path from "node:path";
import { compress, decompress } from "@mongodb-js/zstd";
import writeFileAtomic from "write-file-atomic";

export type ExistingArchiveMeta = {
  path: string;
  sha256: Buffer;
  rawSha256: Buffer | null;
  sizeBytes: number;
};

export type ArchiveInput = {
  archiveDir: string;
  chainId: string;
  blockNumber: bigint;
  blockPayload: Uint8Array;
  receiptsPayload: Uint8Array;
  txIndexPayload: Uint8Array;
  internalTracesPayload?: Uint8Array;
  existingArchive?: ExistingArchiveMeta | null;
  zstdLevel: number;
};

export type ArchiveResult = {
  path: string;
  sha256: Buffer;
  rawSha256: Buffer;
  sizeBytes: number;
  rawBytes: number;
};

export async function archiveBlock(input: ArchiveInput): Promise<ArchiveResult> {
  const raw = buildRaw(
    input.blockPayload,
    input.receiptsPayload,
    input.txIndexPayload,
    input.internalTracesPayload ?? new Uint8Array()
  );
  const rawSha256 = createHash("sha256").update(raw).digest();
  const outPath = buildPath(input.archiveDir, input.chainId, input.blockNumber);
  const existing = await readReusableArchive(input.existingArchive ?? null, outPath, rawSha256);
  if (existing) {
    return {
      path: outPath,
      sha256: existing.sha256,
      rawSha256,
      sizeBytes: existing.sizeBytes,
      rawBytes: raw.length,
    };
  }
  const compressed = await compress(raw, input.zstdLevel);
  const sha256 = createHash("sha256").update(compressed).digest();
  const dir = path.dirname(outPath);
  await fs.mkdir(dir, { recursive: true });
  try {
    await writeFileAtomic(outPath, compressed, { fsync: true });
  } catch (err) {
    const existingAfter = await readReusableArchiveAfterConflict(outPath, rawSha256);
    if (existingAfter) {
      return {
        path: outPath,
        sha256: existingAfter.sha256,
        rawSha256,
        sizeBytes: existingAfter.sizeBytes,
        rawBytes: raw.length,
      };
    }
    throw err;
  }
  return {
    path: outPath,
    sha256,
    rawSha256,
    sizeBytes: compressed.length,
    rawBytes: raw.length,
  };
}

function buildRaw(
  block: Uint8Array,
  receipts: Uint8Array,
  txIndex: Uint8Array,
  internalTraces: Uint8Array
): Buffer {
  const blockLen = toU32(block.length, "block_len");
  const receiptsLen = toU32(receipts.length, "receipts_len");
  const txIndexLen = toU32(txIndex.length, "tx_index_len");
  const internalTracesLen = toU32(internalTraces.length, "internal_traces_len");
  const total = 4 + blockLen + 4 + receiptsLen + 4 + txIndexLen + 4 + internalTracesLen;
  const out = Buffer.allocUnsafe(total);
  let offset = 0;
  offset = writeLen(out, offset, blockLen);
  Buffer.from(block).copy(out, offset);
  offset += blockLen;
  offset = writeLen(out, offset, receiptsLen);
  Buffer.from(receipts).copy(out, offset);
  offset += receiptsLen;
  offset = writeLen(out, offset, txIndexLen);
  Buffer.from(txIndex).copy(out, offset);
  offset += txIndexLen;
  offset = writeLen(out, offset, internalTracesLen);
  Buffer.from(internalTraces).copy(out, offset);
  offset += internalTracesLen;
  if (offset !== total) {
    throw new Error("buildRaw size mismatch");
  }
  return out;
}

function isCurrentArchiveRaw(raw: Uint8Array): boolean {
  try {
    let offset = 0;
    for (let segment = 0; segment < 4; segment += 1) {
      if (offset + 4 > raw.length) {
        return false;
      }
      const len = Buffer.from(raw).readUInt32BE(offset);
      offset += 4;
      if (offset + len > raw.length) {
        return false;
      }
      offset += len;
    }
    return offset === raw.length;
  } catch {
    return false;
  }
}

function writeLen(buf: Buffer, offset: number, len: number): number {
  buf.writeUInt32BE(len, offset);
  return offset + 4;
}

function toU32(value: number, label: string): number {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff) {
    throw new Error(`${label} out of range`);
  }
  return value;
}

function buildPath(baseDir: string, chainId: string, blockNumber: bigint): string {
  const fileName = `${blockNumber.toString()}.bundle.zst`;
  return path.join(baseDir, chainId, fileName);
}

async function readReusableArchive(
  existingArchive: ExistingArchiveMeta | null,
  outPath: string,
  currentRawSha256: Buffer
): Promise<{ sha256: Buffer; sizeBytes: number } | null> {
  if (!existingArchive || existingArchive.path !== outPath || !existingArchive.rawSha256) {
    return null;
  }
  if (!existingArchive.rawSha256.equals(currentRawSha256)) {
    return null;
  }
  try {
    const stat = await fs.stat(outPath);
    if (!stat.isFile()) {
      return null;
    }
    const data = await fs.readFile(outPath);
    return {
      sha256: createHash("sha256").update(data).digest(),
      sizeBytes: stat.size,
    };
  } catch {
    return null;
  }
}

async function readReusableArchiveAfterConflict(
  outPath: string,
  currentRawSha256: Buffer
): Promise<{ sha256: Buffer; sizeBytes: number } | null> {
  try {
    const stat = await fs.stat(outPath);
    if (!stat.isFile()) {
      return null;
    }
    const data = await fs.readFile(outPath);
    const raw = await decompress(data);
    if (!isCurrentArchiveRaw(raw)) {
      return null;
    }
    const rawSha256 = createHash("sha256").update(raw).digest();
    if (!rawSha256.equals(currentRawSha256)) {
      return null;
    }
    return {
      sha256: createHash("sha256").update(data).digest(),
      sizeBytes: stat.size,
    };
  } catch {
    return null;
  }
}
