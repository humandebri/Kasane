/// <reference path="./globals.d.ts" />
// どこで: indexerコミット / 何を: payload decode→archive→DB保存 / なぜ: 失敗時の記録を一箇所に集約するため

import { archiveBlock } from "./archiver";
import { cursorToJson } from "./cursor";
import { decodeBlockPayload, decodeReceiptStatusPayload, decodeTxIndexPayload } from "./decode";
import { IndexerDb } from "./db";
import { Config } from "./config";
import { Cursor, ExportResponse } from "./types";
import { finalizePayloads, Pending } from "./worker_pending";
import { errMessage, logFatal, logInfo } from "./worker_log";
import { toDayKey } from "./worker_utils";
import type { ArchiveResult } from "./archiver";
import type { BlockInfo, ReceiptStatusInfo, TxIndexInfo } from "./decode";

export async function commitPending(params: {
  config: Config;
  db: IndexerDb;
  response: ExportResponse;
  previousCursor: Cursor | null;
  cursor: Cursor;
  headNumber: bigint;
  pending: Pending;
  lastSizeDay: number | null;
}): Promise<{ lastSizeDay: number | null } | null> {
  let blockInfo: BlockInfo | null = null;
  let txIndex: TxIndexInfo[] | null = null;
  let receiptStatuses: ReceiptStatusInfo[] | null = null;
  const payloads = finalizePayloads(params.pending);
  try {
    blockInfo = decodeBlockPayload(payloads[0]);
    receiptStatuses = decodeReceiptStatusPayload(payloads[1]);
    txIndex = decodeTxIndexPayload(payloads[2]);
  } catch (err) {
    logFatal(
      params.config.chainId,
      "Decode",
      params.previousCursor,
      params.headNumber,
      params.response,
      null,
      errMessage(err),
      err
    );
    process.exit(1);
  }
  if (!blockInfo || !txIndex || !receiptStatuses) {
    logFatal(
      params.config.chainId,
      "Decode",
      params.previousCursor,
      params.headNumber,
      params.response,
      null,
      "decode result missing"
    );
    process.exit(1);
  }
  const receiptStatusByTxHash = new Map<string, number>();
  for (const receipt of receiptStatuses) {
    receiptStatusByTxHash.set(receipt.txHash.toString("hex"), receipt.status);
  }
  const txHashes = new Set<string>();
  for (const entry of txIndex) {
    txHashes.add(entry.txHash.toString("hex"));
  }
  if (receiptStatusByTxHash.size !== txHashes.size) {
    logFatal(
      params.config.chainId,
      "Decode",
      params.previousCursor,
      params.headNumber,
      params.response,
      null,
      `receipt status count mismatch: tx_index=${txHashes.size} receipts=${receiptStatusByTxHash.size}`
    );
    process.exit(1);
  }
  for (const txHash of txHashes) {
    if (!receiptStatusByTxHash.has(txHash)) {
      logFatal(
        params.config.chainId,
        "Decode",
        params.previousCursor,
        params.headNumber,
        params.response,
        null,
        `receipt status missing for tx_hash=${txHash}`
      );
      process.exit(1);
    }
  }
  for (const txHash of receiptStatusByTxHash.keys()) {
    if (!txHashes.has(txHash)) {
      logFatal(
        params.config.chainId,
        "Decode",
        params.previousCursor,
        params.headNumber,
        params.response,
        null,
        `receipt status has unknown tx_hash=${txHash}`
      );
      process.exit(1);
    }
  }
  let archive: ArchiveResult | null = null;
  try {
    archive = await archiveBlock({
      archiveDir: params.config.archiveDir,
      chainId: params.config.chainId,
      blockNumber: blockInfo.number,
      blockPayload: payloads[0],
      receiptsPayload: payloads[1],
      txIndexPayload: payloads[2],
      zstdLevel: params.config.zstdLevel,
    });
  } catch (err) {
    logFatal(
      params.config.chainId,
      "ArchiveIO",
      params.previousCursor,
      params.headNumber,
      params.response,
      null,
      errMessage(err),
      err
    );
    process.exit(1);
  }
  if (!archive) {
    logFatal(
      params.config.chainId,
      "ArchiveIO",
      params.previousCursor,
      params.headNumber,
      params.response,
      null,
      "archive result missing"
    );
    process.exit(1);
  }
  const blocksIngested = 1;
  const metricsDay = toDayKey();
  const updateSizes = params.lastSizeDay !== metricsDay;
  let lastSizeDay = params.lastSizeDay;
  try {
    await params.db.transaction(async (client) => {
      await client.query(
        "INSERT INTO blocks(number, hash, timestamp, tx_count) VALUES($1, $2, $3, $4) ON CONFLICT(number) DO UPDATE SET hash = excluded.hash, timestamp = excluded.timestamp, tx_count = excluded.tx_count",
        [blockInfo.number, blockInfo.blockHash, blockInfo.timestamp, blockInfo.txIds.length]
      );
      for (const entry of txIndex) {
        const receiptStatus = receiptStatusByTxHash.get(entry.txHash.toString("hex"));
        if (receiptStatus === undefined) {
          throw new Error(`receipt status missing after validation for tx_hash=${entry.txHash.toString("hex")}`);
        }
        await client.query(
          "INSERT INTO txs(tx_hash, block_number, tx_index, caller_principal, from_address, to_address, receipt_status) VALUES($1, $2, $3, $4, $5, $6, $7) ON CONFLICT(tx_hash) DO UPDATE SET block_number = excluded.block_number, tx_index = excluded.tx_index, caller_principal = excluded.caller_principal, from_address = excluded.from_address, to_address = excluded.to_address, receipt_status = excluded.receipt_status",
          [
            entry.txHash,
            entry.blockNumber,
            entry.txIndex,
            entry.callerPrincipal,
            entry.fromAddress,
            entry.toAddress,
            receiptStatus,
          ]
        );
      }
      await client.query(
        "INSERT INTO archive_parts(block_number, path, sha256, size_bytes, raw_bytes, created_at) VALUES($1, $2, $3, $4, $5, $6) " +
          "ON CONFLICT(block_number) DO UPDATE SET path = excluded.path, sha256 = excluded.sha256, size_bytes = excluded.size_bytes, raw_bytes = excluded.raw_bytes, created_at = excluded.created_at",
        [blockInfo.number, archive.path, archive.sha256, archive.sizeBytes, archive.rawBytes, Date.now()]
      );
      await client.query(
        "INSERT INTO meta(key, value) VALUES($1, $2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        ["cursor", cursorToJson(params.cursor)]
      );
      await client.query(
        "INSERT INTO metrics_daily(day, raw_bytes, compressed_bytes, archive_bytes, blocks_ingested, errors) VALUES($1, $2, $3, $4, $5, $6) " +
          "ON CONFLICT(day) DO UPDATE SET raw_bytes = metrics_daily.raw_bytes + excluded.raw_bytes, compressed_bytes = metrics_daily.compressed_bytes + excluded.compressed_bytes, archive_bytes = COALESCE(excluded.archive_bytes, metrics_daily.archive_bytes), blocks_ingested = metrics_daily.blocks_ingested + excluded.blocks_ingested, errors = metrics_daily.errors + excluded.errors",
        [metricsDay, archive.rawBytes, archive.sizeBytes, null, blocksIngested, 0]
      );
      await client.query(
        "INSERT INTO meta(key, value) VALUES($1, $2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        ["last_head", params.headNumber.toString()]
      );
      await client.query(
        "INSERT INTO meta(key, value) VALUES($1, $2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        ["last_ingest_at", Date.now().toString()]
      );
    });
    logInfo(params.config.chainId, "commit_block", {
      block_number: blockInfo.number.toString(),
      tx_count: blockInfo.txIds.length,
      tx_id: blockInfo.txIds.length > 0 ? blockInfo.txIds[0].toString("hex") : null,
      cursor_prev: params.previousCursor
        ? {
            block_number: params.previousCursor.block_number.toString(),
            segment: params.previousCursor.segment,
            byte_offset: params.previousCursor.byte_offset,
          }
        : null,
      cursor_next: {
        block_number: params.cursor.block_number.toString(),
        segment: params.cursor.segment,
        byte_offset: params.cursor.byte_offset,
      },
    });
  } catch (err) {
    logFatal(
      params.config.chainId,
      "Db",
      params.previousCursor,
      params.headNumber,
      params.response,
      archive,
      errMessage(err),
      err
    );
    process.exit(1);
  }
  if (updateSizes) {
    let archiveBytesToday: number | null = null;
    try {
      archiveBytesToday = await params.db.getArchiveBytesSum();
    } catch {
      archiveBytesToday = null;
    }
    try {
      await params.db.addMetrics(metricsDay, 0, 0, 0, 0, archiveBytesToday);
      lastSizeDay = metricsDay;
    } catch {
      // サイズ計測の更新失敗は取り込み成功を壊さない
    }
  }
  return { lastSizeDay };
}
