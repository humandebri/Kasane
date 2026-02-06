/// <reference path="./globals.d.ts" />
// どこで: indexerコミット / 何を: payload decode→archive→DB保存 / なぜ: 失敗時の記録を一箇所に集約するため

import { archiveBlock } from "./archiver";
import { decodeBlockPayload, decodeTxIndexPayload } from "./decode";
import { IndexerDb } from "./db";
import { Config } from "./config";
import { Cursor, ExportResponse } from "./types";
import { finalizePayloads, Pending } from "./worker_pending";
import { errMessage, logFatal } from "./worker_log";
import { getFileSize, toDayKey } from "./worker_utils";
import type { ArchiveResult } from "./archiver";
import type { BlockInfo, TxIndexInfo } from "./decode";

export async function commitPending(params: {
  config: Config;
  db: IndexerDb;
  response: ExportResponse;
  previousCursor: Cursor | null;
  cursor: Cursor | null;
  headNumber: bigint;
  pending: Pending;
  lastSizeDay: number | null;
}): Promise<{ lastSizeDay: number | null } | null> {
  let blockInfo: BlockInfo | null = null;
  let txIndex: TxIndexInfo[] | null = null;
  const payloads = finalizePayloads(params.pending);
  try {
    blockInfo = decodeBlockPayload(payloads[0]);
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
  if (!blockInfo || !txIndex) {
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
  let sqliteBytesToday: number | null = null;
  let archiveBytesToday: number | null = null;
  let lastSizeDay = params.lastSizeDay;
  if (updateSizes) {
    sqliteBytesToday = await getFileSize(params.config.dbPath);
    archiveBytesToday = params.db.getArchiveBytesSum();
    lastSizeDay = metricsDay;
  }
  try {
    params.db.transaction(() => {
      params.db.upsertBlock({
        number: blockInfo.number,
        hash: blockInfo.blockHash,
        timestamp: blockInfo.timestamp,
        tx_count: blockInfo.txIds.length,
      });
      for (const entry of txIndex) {
        params.db.upsertTx({
          tx_hash: entry.txHash,
          block_number: entry.blockNumber,
          tx_index: entry.txIndex,
        });
      }
      params.db.addArchive({
        blockNumber: blockInfo.number,
        path: archive.path,
        sha256: archive.sha256,
        sizeBytes: archive.sizeBytes,
        rawBytes: archive.rawBytes,
        createdAt: Date.now(),
      });
      if (!params.cursor) {
        throw new Error("cursor missing on commit");
      }
      params.db.setCursor(params.cursor);
      params.db.addMetrics(
        metricsDay,
        archive.rawBytes,
        archive.sizeBytes,
        blocksIngested,
        0,
        sqliteBytesToday,
        archiveBytesToday
      );
      params.db.setMeta("last_head", params.headNumber.toString());
      params.db.setMeta("last_ingest_at", Date.now().toString());
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
  return { lastSizeDay };
}
