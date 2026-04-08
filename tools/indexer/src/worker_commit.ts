/// <reference path="./globals.d.ts" />
// どこで: indexerコミット / 何を: payload decode→archive→DB保存 / なぜ: 失敗時の記録を一箇所に集約するため

import { archiveBlock } from "./archiver.js";
import { cursorToJson } from "./cursor.js";
import {
  decodeBlockPayload,
  decodeInternalTracesPayload,
  decodeReceiptsPayload,
  decodeTxIndexPayload,
} from "./decode.js";
import { IndexerDb } from "./db.js";
import { Config } from "./config.js";
import { Cursor, ExportResponse } from "./types.js";
import { finalizePayloads, Pending } from "./worker_pending.js";
import { errMessage, logFatal, logInfo, logWarn } from "./worker_log.js";
import { isTokenTransferAmountSupported } from "./worker_commit_guard.js";
import { toDayKey } from "./worker_utils.js";
import type { ArchiveResult } from "./archiver.js";
import type {
  BlockInfo,
  DecodedInternalTracesInfo,
  Erc20TransferInfo,
  ReceiptStatusInfo,
  TxIndexInfo,
} from "./decode.js";

export async function commitPending(params: {
  config: Config;
  db: IndexerDb;
  response: ExportResponse;
  previousCursor: Cursor | null;
  cursor: Cursor;
  headNumber: bigint;
  pending: Pending;
  lastSizeDay: number | null;
  getTxMetaByTxId: (txId: Uint8Array) => Promise<{ input: Uint8Array | null; ethTxHash: Uint8Array | null }>;
}): Promise<{ lastSizeDay: number | null } | null> {
  let blockInfo: BlockInfo | null = null;
  let txIndex: TxIndexInfo[] | null = null;
  let receiptStatuses: ReceiptStatusInfo[] | null = null;
  let tokenTransfers: Erc20TransferInfo[] | null = null;
  let internalTraces: DecodedInternalTracesInfo | null = null;
  let skippedTokenTransfersMalformed = 0;
  const payloads = finalizePayloads(params.pending);
  try {
    blockInfo = decodeBlockPayload(payloads[0]);
    const decodedReceipts = decodeReceiptsPayload(payloads[1]);
    receiptStatuses = decodedReceipts.statuses;
    tokenTransfers = decodedReceipts.tokenTransfers;
    skippedTokenTransfersMalformed = decodedReceipts.skippedTokenTransfers;
    txIndex = decodeTxIndexPayload(payloads[2]);
    internalTraces = decodeInternalTracesPayload(payloads[3]);
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
  if (!blockInfo || !txIndex || !receiptStatuses || !tokenTransfers || !internalTraces) {
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
  const receiptStatusByTxHash = new Map<string, {
    status: number;
    contractAddress: Buffer | null;
    blockNumber: bigint;
    txIndex: number;
  }>();
  for (const receipt of receiptStatuses) {
    receiptStatusByTxHash.set(receipt.txHash.toString("hex"), {
      status: receipt.status,
      contractAddress: receipt.contractAddress,
      blockNumber: receipt.blockNumber,
      txIndex: receipt.txIndex,
    });
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
  const txInputByTxHash = await fetchTxInputByHash(txIndex, params.getTxMetaByTxId, params.config.chainId);
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
    const existingArchive = await params.db.getArchivePart(blockInfo.number);
    archive = await archiveBlock({
      archiveDir: params.config.archiveDir,
      chainId: params.config.chainId,
      blockNumber: blockInfo.number,
      blockPayload: payloads[0],
      receiptsPayload: payloads[1],
      txIndexPayload: payloads[2],
      internalTracesPayload: payloads[3],
      existingArchive,
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
      const existingBlock = await client.query(
        "SELECT 1 FROM blocks WHERE number = $1 FOR UPDATE",
        [blockInfo.number]
      );
      const isNewBlock = (existingBlock.rowCount ?? 0) === 0;
      const metricRawBytes = isNewBlock ? archive.rawBytes : 0;
      const metricCompressedBytes = isNewBlock ? archive.sizeBytes : 0;
      const blocksIngested = isNewBlock ? 1 : 0;
      await client.query(
        "INSERT INTO blocks(number, hash, timestamp, tx_count, gas_used) VALUES($1, $2, $3, $4, $5) ON CONFLICT(number) DO UPDATE SET hash = excluded.hash, timestamp = excluded.timestamp, tx_count = excluded.tx_count, gas_used = excluded.gas_used",
        [blockInfo.number, blockInfo.blockHash, blockInfo.timestamp, blockInfo.txIds.length, blockInfo.gasUsed]
      );
      const blockTxHashes = txIndex.map((entry) => entry.txHash);
      for (const entry of txIndex) {
        const receiptStatus = receiptStatusByTxHash.get(entry.txHash.toString("hex"));
        if (!receiptStatus) {
          throw new Error(`receipt status missing after validation for tx_hash=${entry.txHash.toString("hex")}`);
        }
        await client.query(
          "INSERT INTO txs(tx_hash, eth_tx_hash, block_number, tx_index, caller_principal, from_address, to_address, tx_input, tx_selector, receipt_status, internal_trace_failed, internal_trace_truncated, internal_trace_captured_count, internal_trace_total_count) VALUES($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, false, false, null, null) ON CONFLICT(tx_hash) DO UPDATE SET eth_tx_hash = COALESCE(excluded.eth_tx_hash, txs.eth_tx_hash), block_number = excluded.block_number, tx_index = excluded.tx_index, caller_principal = excluded.caller_principal, from_address = excluded.from_address, to_address = excluded.to_address, tx_input = COALESCE(excluded.tx_input, txs.tx_input), tx_selector = excluded.tx_selector, receipt_status = excluded.receipt_status, internal_trace_failed = excluded.internal_trace_failed, internal_trace_truncated = excluded.internal_trace_truncated, internal_trace_captured_count = excluded.internal_trace_captured_count, internal_trace_total_count = excluded.internal_trace_total_count",
          [
            entry.txHash,
            entry.ethTxHash,
            entry.blockNumber,
            entry.txIndex,
            entry.callerPrincipal,
            entry.fromAddress,
            entry.toAddress,
            txInputByTxHash.get(entry.txHash.toString("hex")) ?? null,
            entry.txSelector,
            receiptStatus.status,
          ]
        );
        await client.query(
          "INSERT INTO tx_receipts_index(tx_hash, contract_address, status, block_number, tx_index) VALUES($1, $2, $3, $4, $5) ON CONFLICT(tx_hash) DO UPDATE SET contract_address = excluded.contract_address, status = excluded.status, block_number = excluded.block_number, tx_index = excluded.tx_index",
          [
            entry.txHash,
            receiptStatus.contractAddress,
            receiptStatus.status,
            receiptStatus.blockNumber,
            receiptStatus.txIndex,
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
      if (blockTxHashes.length > 0) {
        await client.query("DELETE FROM internal_transactions WHERE tx_hash = ANY($1::bytea[])", [blockTxHashes]);
      }
      for (const internalTraceTx of internalTraces.txs) {
        await client.query(
          "UPDATE txs SET internal_trace_failed = $2, internal_trace_truncated = $3, internal_trace_captured_count = $4, internal_trace_total_count = $5 WHERE tx_hash = $1",
          [
            internalTraceTx.txHash,
            internalTraceTx.failed,
            internalTraceTx.truncated,
            internalTraceTx.capturedCount,
            internalTraceTx.totalCount,
          ]
        );
      }
      for (const internalTx of internalTraces.transactions) {
        await client.query(
          "INSERT INTO internal_transactions(tx_hash, block_number, tx_index, trace_id, trace_sort_key, depth, action_type, from_address, to_address, created_contract_address, value_numeric, success, error_code) VALUES($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) ON CONFLICT(tx_hash, trace_id) DO UPDATE SET block_number = excluded.block_number, tx_index = excluded.tx_index, trace_sort_key = excluded.trace_sort_key, depth = excluded.depth, action_type = excluded.action_type, from_address = excluded.from_address, to_address = excluded.to_address, created_contract_address = excluded.created_contract_address, value_numeric = excluded.value_numeric, success = excluded.success, error_code = excluded.error_code",
          [
            internalTx.txHash,
            internalTx.blockNumber,
            internalTx.txIndex,
            internalTx.traceId,
            internalTx.traceSortKey,
            internalTx.depth,
            internalTx.actionType,
            internalTx.fromAddress,
            internalTx.toAddress,
            internalTx.createdContractAddress,
            internalTx.value.toString(),
            internalTx.success,
            internalTx.errorCode,
          ]
        );
      }
      await client.query(
        "INSERT INTO archive_parts(block_number, path, sha256, raw_sha256, size_bytes, raw_bytes, created_at) VALUES($1, $2, $3, $4, $5, $6, $7) " +
          "ON CONFLICT(block_number) DO UPDATE SET path = excluded.path, sha256 = excluded.sha256, raw_sha256 = excluded.raw_sha256, size_bytes = excluded.size_bytes, raw_bytes = excluded.raw_bytes, created_at = excluded.created_at",
        [blockInfo.number, archive.path, archive.sha256, archive.rawSha256, archive.sizeBytes, archive.rawBytes, Date.now()]
      );
      await client.query(
        "INSERT INTO meta(key, value) VALUES($1, $2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        ["cursor", cursorToJson(params.cursor)]
      );
      await client.query(
        "INSERT INTO metrics_daily(day, raw_bytes, compressed_bytes, archive_bytes, blocks_ingested, errors) VALUES($1, $2, $3, $4, $5, $6) " +
          "ON CONFLICT(day) DO UPDATE SET raw_bytes = metrics_daily.raw_bytes + excluded.raw_bytes, compressed_bytes = metrics_daily.compressed_bytes + excluded.compressed_bytes, archive_bytes = COALESCE(excluded.archive_bytes, metrics_daily.archive_bytes), blocks_ingested = metrics_daily.blocks_ingested + excluded.blocks_ingested, errors = metrics_daily.errors + excluded.errors",
        [metricsDay, metricRawBytes, metricCompressedBytes, null, blocksIngested, 0]
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
      internal_trace_total: internalTraces.transactions.length,
      internal_trace_truncated_txs: internalTraces.txs.filter((item) => item.truncated).length,
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

async function fetchTxInputByHash(
  txs: TxIndexInfo[],
  getTxMetaByTxId: (txId: Uint8Array) => Promise<{ input: Uint8Array | null; ethTxHash: Uint8Array | null }>,
  chainId: string
): Promise<Map<string, Buffer | null>> {
  const retryBackoffMs = [20, 60];
  const out = new Map<string, Buffer | null>();
  await Promise.all(
    txs.map(async (tx) => {
      let lastErr: unknown = null;
      try {
        for (let attempt = 0; attempt <= retryBackoffMs.length; attempt += 1) {
          try {
            const meta = await getTxMetaByTxId(tx.txHash);
            out.set(tx.txHash.toString("hex"), meta.input && meta.input.length > 0 ? Buffer.from(meta.input) : null);
            return;
          } catch (err) {
            lastErr = err;
            if (attempt >= retryBackoffMs.length) {
              break;
            }
            const backoffMs = retryBackoffMs[attempt];
            logWarn(chainId, "tx_meta_retry", {
              tx_hash: tx.txHash.toString("hex"),
              retry_count: attempt + 1,
              backoff_ms: backoffMs,
              err: errMessage(err),
            });
            await sleep(backoffMs);
          }
        }
      } catch {
        // no-op: below warning logs on final failure.
      }
      if (lastErr) {
        logWarn(chainId, "tx_meta_unavailable", {
          tx_hash: tx.txHash.toString("hex"),
          err: errMessage(lastErr),
        });
      }
    })
  );
  return out;
}

async function sleep(ms: number): Promise<void> {
  await new Promise<void>((resolve) => {
    setTimeout(resolve, ms);
  });
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
