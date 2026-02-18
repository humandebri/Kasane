/// <reference path="./globals.d.ts" />
// どこで: indexerコミット / 何を: payload decode→archive→DB保存 / なぜ: 失敗時の記録を一箇所に集約するため

import { archiveBlock } from "./archiver";
import { cursorToJson } from "./cursor";
import { decodeBlockPayload, decodeReceiptsPayload, decodeTxIndexPayload } from "./decode";
import { IndexerDb } from "./db";
import { Config } from "./config";
import { Cursor, ExportResponse } from "./types";
import { finalizePayloads, Pending } from "./worker_pending";
import { errMessage, logFatal, logInfo } from "./worker_log";
import { isTokenTransferAmountSupported } from "./worker_commit_guard";
import { toDayKey } from "./worker_utils";
import type { ArchiveResult } from "./archiver";
import type { BlockInfo, Erc20TransferInfo, ReceiptStatusInfo, TxIndexInfo } from "./decode";

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
  let tokenTransfers: Erc20TransferInfo[] | null = null;
  let skippedTokenTransfersMalformed = 0;
  const payloads = finalizePayloads(params.pending);
  try {
    blockInfo = decodeBlockPayload(payloads[0]);
    const decodedReceipts = decodeReceiptsPayload(payloads[1]);
    receiptStatuses = decodedReceipts.statuses;
    tokenTransfers = decodedReceipts.tokenTransfers;
    skippedTokenTransfersMalformed = decodedReceipts.skippedTokenTransfers;
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
  if (!blockInfo || !txIndex || !receiptStatuses || !tokenTransfers) {
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
  const expectedBlockNumber = params.previousCursor ? params.previousCursor.block_number : blockInfo.number;
  if (blockInfo.number !== expectedBlockNumber) {
    logFatal(
      params.config.chainId,
      "InvalidCursor",
      params.previousCursor,
      params.headNumber,
      params.response,
      null,
      `decoded block number mismatch: expected=${expectedBlockNumber.toString()} actual=${blockInfo.number.toString()}`
    );
    process.exit(1);
  }
  const receiptStatusByTxHash = new Map<string, number>();
  for (const receipt of receiptStatuses) {
    receiptStatusByTxHash.set(receipt.txHash.toString("hex"), receipt.status);
  }
  const txHashes = new Set<string>();
  const txPositionByTxHash = new Map<string, { blockNumber: bigint; txIndex: number }>();
  for (const entry of txIndex) {
    const txHashHex = entry.txHash.toString("hex");
    txHashes.add(txHashHex);
    txPositionByTxHash.set(txHashHex, { blockNumber: entry.blockNumber, txIndex: entry.txIndex });
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
  let skippedTokenTransfersAmount = 0;
  let skippedTokenTransfersDb = 0;
  let persistedTokenTransfers = 0;
  let tokenTransferSavepointSupported: boolean | null = null;
  const tokenTransferRows: Array<Erc20TransferInfo & { txPosition: { blockNumber: bigint; txIndex: number } }> = [];
  for (const transfer of tokenTransfers) {
    const txHashHex = transfer.txHash.toString("hex");
    const txPosition = txPositionByTxHash.get(txHashHex);
    if (!txPosition) {
      logFatal(
        params.config.chainId,
        "Decode",
        params.previousCursor,
        params.headNumber,
        params.response,
        null,
        `token transfer has unknown tx_hash=${txHashHex}`
      );
      process.exit(1);
    }
    tokenTransferRows.push({ ...transfer, txPosition });
  }
  try {
    await params.db.transaction(async (client) => {
      await client.query(
        "INSERT INTO blocks(number, hash, timestamp, tx_count, gas_used) VALUES($1, $2, $3, $4, $5) ON CONFLICT(number) DO UPDATE SET hash = excluded.hash, timestamp = excluded.timestamp, tx_count = excluded.tx_count, gas_used = excluded.gas_used",
        [blockInfo.number, blockInfo.blockHash, blockInfo.timestamp, blockInfo.txIds.length, blockInfo.gasUsed]
      );
      for (const entry of txIndex) {
        const receiptStatus = receiptStatusByTxHash.get(entry.txHash.toString("hex"));
        if (receiptStatus === undefined) {
          throw new Error(`receipt status missing after validation for tx_hash=${entry.txHash.toString("hex")}`);
        }
        await client.query(
          "INSERT INTO txs(tx_hash, block_number, tx_index, caller_principal, from_address, to_address, tx_selector, receipt_status) VALUES($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT(tx_hash) DO UPDATE SET block_number = excluded.block_number, tx_index = excluded.tx_index, caller_principal = excluded.caller_principal, from_address = excluded.from_address, to_address = excluded.to_address, tx_selector = excluded.tx_selector, receipt_status = excluded.receipt_status",
          [
            entry.txHash,
            entry.blockNumber,
            entry.txIndex,
            entry.callerPrincipal,
            entry.fromAddress,
            entry.toAddress,
            entry.txSelector,
            receiptStatus,
          ]
        );
      }
      for (const transfer of tokenTransferRows) {
        if (!isTokenTransferAmountSupported(transfer.amount)) {
          skippedTokenTransfersAmount += 1;
          continue;
        }
        if (tokenTransferSavepointSupported !== false) {
          try {
            await client.query("SAVEPOINT token_transfer_row");
            tokenTransferSavepointSupported = true;
          } catch (err) {
            if (tokenTransferSavepointSupported === null && isSavepointUnsupportedError(err)) {
              tokenTransferSavepointSupported = false;
            } else {
              throw err;
            }
          }
        }
        if (tokenTransferSavepointSupported === false) {
          try {
            await upsertTokenTransferRow(client, transfer);
            persistedTokenTransfers += 1;
          } catch {
            // token_transfersは補助インデックス。失敗時はブロック取り込みを継続する。
            skippedTokenTransfersDb += 1;
          }
          continue;
        }
        try {
          await upsertTokenTransferRow(client, transfer);
          await client.query("RELEASE SAVEPOINT token_transfer_row");
          persistedTokenTransfers += 1;
        } catch {
          // token_transfersは補助インデックス。失敗時はブロック取り込みを継続する。
          skippedTokenTransfersDb += 1;
          await client.query("ROLLBACK TO SAVEPOINT token_transfer_row");
          await client.query("RELEASE SAVEPOINT token_transfer_row");
        }
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
      token_transfer_total: tokenTransfers.length,
      token_transfer_persisted: persistedTokenTransfers,
      token_transfer_skipped_malformed: skippedTokenTransfersMalformed,
      token_transfer_skipped_amount: skippedTokenTransfersAmount,
      token_transfer_skipped_db: skippedTokenTransfersDb,
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

async function upsertTokenTransferRow(
  client: { query: (text: string, values?: unknown[]) => Promise<unknown> },
  transfer: Erc20TransferInfo & { txPosition: { blockNumber: bigint; txIndex: number } }
): Promise<void> {
  await client.query(
    "INSERT INTO token_transfers(tx_hash, block_number, tx_index, log_index, token_address, from_address, to_address, amount_numeric) VALUES($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT(tx_hash, log_index) DO UPDATE SET block_number = excluded.block_number, tx_index = excluded.tx_index, token_address = excluded.token_address, from_address = excluded.from_address, to_address = excluded.to_address, amount_numeric = excluded.amount_numeric",
    [
      transfer.txHash,
      transfer.txPosition.blockNumber,
      transfer.txPosition.txIndex,
      transfer.logIndex,
      transfer.tokenAddress,
      transfer.fromAddress,
      transfer.toAddress,
      transfer.amount.toString(),
    ]
  );
}

function isSavepointUnsupportedError(err: unknown): boolean {
  const message = err instanceof Error ? err.message : String(err);
  return message.includes("SAVEPOINT") && message.includes("syntax");
}
