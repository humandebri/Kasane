/// <reference path="./globals.d.ts" />
// どこで: indexerメインループ / 何を: pull→検証→DBコミット / なぜ: 仕様のコミット境界を守るため

import { Config, sleep } from "./config";
import { IndexerDb } from "./db";
import { runArchiveGcWithMode } from "./archive_gc";
import { Cursor, ExportError, ExportResponse, MetricsView, PruneStatusView, Result } from "./types";
import {
  applyChunk,
  enforceNextCursor,
  finalizePayloads,
  newPending,
  newPendingFromChunk,
  Pending,
  totalChunkBytes,
} from "./worker_pending";
import { errMessage, logFatal, logIdle, logInfo, logRetry, logWarn } from "./worker_log";
import {
  jsonStringifyBigInt,
  nextBackoff,
  setupFatalHandlers,
  setupSignalHandlers,
  toDayKey,
} from "./worker_utils";
import { classifyExportError } from "./worker_errors";
import { commitPending } from "./worker_commit";
import { decodeBlockPayload } from "./decode";

export async function runWorkerWithDeps(
  config: Config,
  db: IndexerDb,
  client: {
    getHeadNumber: () => Promise<bigint>;
    exportBlocks: (cursor: Cursor | null, maxBytes: number) => Promise<Result<ExportResponse, ExportError>>;
    getPruneStatus: () => Promise<PruneStatusView>;
    getMetrics: (window: bigint) => Promise<MetricsView>;
  },
  options: { skipGc: boolean }
): Promise<void> {
  if (!options.skipGc) {
    try {
      await runArchiveGcWithMode(db, config.archiveDir, config.chainId, config.archiveGcDeleteOrphans);
    } catch (err) {
      logWarn(config.chainId, "archive_gc_failed", { message: errMessage(err) });
    }
  }
  let cursor: Cursor | null = await db.getCursor();
  if (cursor && cursor.segment > config.maxSegment) {
    logFatal(
      config.chainId,
      "InvalidCursor",
      cursor,
      null,
      null,
      null,
      `stored cursor segment exceeds INDEXER_MAX_SEGMENT (segment=${cursor.segment}, max=${config.maxSegment}); possible server/indexer schema mismatch`
    );
    process.exit(1);
  }
  let lastHead: bigint | null = null;
  let backoffMs = config.backoffInitialMs;
  let pending: Pending | null = null;
  let retryCount = 0;
  let lastIdleLogAt = 0;
  let stopRequested = false;
  let lastPruneStatusAt = 0;
  let lastOpsMetricsAt = 0;
  let lastSizeDay: number | null = null;

  const teardownSignals = setupSignalHandlers(config.chainId, () => {
    stopRequested = true;
  });
  const teardownFatalHandlers = setupFatalHandlers((err) => {
    logFatal(config.chainId, "Unknown", cursor, lastHead, null, null, "uncaught", err);
    process.exit(1);
  });

  try {
    for (;;) {
      if (stopRequested) {
        logInfo(config.chainId, "stop_requested", { message: "exiting loop" });
        break;
      }
      let headNumber: bigint;
      try {
        headNumber = await client.getHeadNumber();
      } catch (err) {
        retryCount += 1;
        logRetry(config.chainId, backoffMs, retryCount, "head_fetch_failed", err);
        await sleep(backoffMs);
        backoffMs = nextBackoff(backoffMs, config.backoffMaxMs);
        continue;
      }
      lastHead = headNumber;

      const nowMs = Date.now();
      if (config.pruneStatusPollMs > 0 && nowMs - lastPruneStatusAt >= config.pruneStatusPollMs) {
        lastPruneStatusAt = nowMs;
        try {
          const status = await client.getPruneStatus();
          const payload = { v: 1, fetched_at_ms: nowMs, status };
          await db.transaction(async (client) => {
            await client.query(
              "INSERT INTO meta(key, value) VALUES($1, $2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
              ["prune_status", jsonStringifyBigInt(payload)]
            );
            await client.query(
              "INSERT INTO meta(key, value) VALUES($1, $2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
              ["prune_status_at", String(nowMs)]
            );
            await client.query(
              "INSERT INTO meta(key, value) VALUES($1, $2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
              ["need_prune", status.need_prune ? "1" : "0"]
            );
          });
        } catch (err) {
          logWarn(config.chainId, "prune_status_failed", {
            poll_ms: config.pruneStatusPollMs,
            err: errMessage(err),
          });
        }
      }

      if (config.opsMetricsPollMs > 0 && nowMs - lastOpsMetricsAt >= config.opsMetricsPollMs) {
        lastOpsMetricsAt = nowMs;
        try {
          const metrics = await client.getMetrics(128n);
          const retentionCutoffMs = BigInt(nowMs) - 14n * 24n * 60n * 60n * 1000n;
          await db.addOpsMetricsSample({
            sampledAtMs: BigInt(nowMs),
            queueLen: metrics.queue_len,
            totalSubmitted: metrics.total_submitted,
            totalIncluded: metrics.total_included,
            totalDropped: metrics.total_dropped,
            dropCountsJson: jsonStringifyBigInt(metrics.drop_counts),
            retentionCutoffMs,
          });
        } catch (err) {
          logWarn(config.chainId, "ops_metrics_failed", {
            poll_ms: config.opsMetricsPollMs,
            err: errMessage(err),
          });
        }
      }
      let result: Result<ExportResponse, ExportError>;
      try {
        result = await client.exportBlocks(cursor, config.maxBytes);
      } catch (err) {
        retryCount += 1;
        logRetry(config.chainId, backoffMs, retryCount, "export_blocks_failed", err);
        await sleep(backoffMs);
        backoffMs = nextBackoff(backoffMs, config.backoffMaxMs);
        continue;
      }

      if ("Err" in result) {
        if ("Pruned" in result.Err) {
          if (headNumber === 0n) {
            retryCount = 0;
            const idleSleep = config.idlePollMs;
            logIdle(config.chainId, cursor, headNumber, idleSleep, lastIdleLogAt, (ts) => {
              lastIdleLogAt = ts;
            });
            await sleep(idleSleep);
            backoffMs = config.backoffInitialMs;
            continue;
          }
          cursor = cursorFromPruned(result.Err.Pruned.pruned_before_block, headNumber);
          pending = null;
          retryCount = 0;
          backoffMs = config.backoffInitialMs;
          try {
            await db.setCursor(cursor);
          } catch (err) {
            logWarn(config.chainId, "pruned_rebase_cursor_persist_failed", { err: errMessage(err) });
          }
          try {
            await recordPrunedRebase(db);
          } catch (err) {
            logWarn(config.chainId, "pruned_rebase_metrics_failed", { err: errMessage(err) });
          }
          logWarn(config.chainId, "bootstrap_cursor_from_pruned", {
            head: headNumber.toString(),
            pruned_before_block: result.Err.Pruned.pruned_before_block.toString(),
            next_cursor: `${cursor.block_number.toString()}:0:0`,
          });
          continue;
        }
        // canisterのcursor未指定は block 0 から始まるが、block 0 は export対象外で
        // MissingData になる場合がある。初回同期時は最小有効ブロックへbootstrapして継続する。
        if (!cursor && "MissingData" in result.Err) {
          if (headNumber === 0n) {
            retryCount = 0;
            const idleSleep = config.idlePollMs;
            logIdle(config.chainId, cursor, headNumber, idleSleep, lastIdleLogAt, (ts) => {
              lastIdleLogAt = ts;
            });
            await sleep(idleSleep);
            backoffMs = config.backoffInitialMs;
            continue;
          }
          cursor = await bootstrapCursorFromMissingData(client, headNumber, config.chainId);
          logWarn(config.chainId, "bootstrap_cursor_from_missing_data", {
            head: headNumber.toString(),
            next_cursor: `${cursor.block_number.toString()}:0:0`,
          });
          continue;
        }
        const classified = await classifyExportError(result.Err, db);
        logFatal(config.chainId, classified.kind, cursor, headNumber, null, null, classified.message);
        process.exit(1);
      } else {
        const response = result.Ok;
        if (response.chunks.length === 0) {
          retryCount = 0;
          const idleSleep = config.idlePollMs;
          logIdle(config.chainId, cursor, headNumber, idleSleep, lastIdleLogAt, (ts) => {
            lastIdleLogAt = ts;
          });
          await sleep(idleSleep);
          backoffMs = config.backoffInitialMs;
          continue;
        }
        backoffMs = config.backoffInitialMs;
        retryCount = 0;
        lastIdleLogAt = 0;
        if (totalChunkBytes(response.chunks) > config.maxBytes) {
          logFatal(
            config.chainId,
            "InvalidCursor",
            cursor,
            headNumber,
            response,
            null,
            "chunk bytes exceed max_bytes"
          );
          process.exit(1);
        }

        if (!response.next_cursor) {
          logFatal(
            config.chainId,
            "InvalidCursor",
            cursor,
            headNumber,
            response,
            null,
            "missing next_cursor"
          );
          process.exit(1);
        }

      if (cursor) {
        try {
          enforceNextCursor(response, cursor, config.maxSegment);
        } catch (err) {
          logFatal(
            config.chainId,
            "InvalidCursor",
            cursor,
            headNumber,
            response,
            null,
            errMessage(err),
            err
          );
          process.exit(1);
        }
      }

      const previousCursor = cursor;
      const finalCursor = response.next_cursor;
      let streamCursor = previousCursor;
      let activePending: Pending | null = pending;

      if (!activePending) {
        if (previousCursor) {
          activePending = newPending(previousCursor);
        } else {
          activePending = newPendingFromChunk(response.chunks[0]);
        }
      }

      for (let i = 0; i < response.chunks.length; i += 1) {
        const chunk = response.chunks[i];
        if (!activePending) {
          activePending = newPendingFromChunk(chunk);
        }
        const isFirstChunk = i === 0;
        if (streamCursor) {
          if (chunk.segment !== streamCursor.segment || chunk.start !== streamCursor.byte_offset) {
            logFatal(
              config.chainId,
              "InvalidCursor",
              previousCursor,
              headNumber,
              response,
              null,
              "cursor and chunk stream mismatch"
            );
            process.exit(1);
          }
        } else if (isFirstChunk) {
          if (chunk.segment !== activePending.segment || chunk.start !== activePending.offset) {
            logFatal(
              config.chainId,
              "InvalidCursor",
              previousCursor,
              headNumber,
              response,
              null,
              "initial chunk mismatch without cursor"
            );
            process.exit(1);
          }
        }
        try {
          applyChunk(activePending, chunk);
        } catch (err) {
          logFatal(
            config.chainId,
            "InvalidCursor",
            previousCursor,
            headNumber,
            response,
            null,
            errMessage(err),
            err
          );
          process.exit(1);
        }
        if (activePending.complete) {
          let commitCursor: Cursor;
          if (streamCursor) {
            commitCursor = {
              block_number: streamCursor.block_number + 1n,
              segment: 0,
              byte_offset: 0,
            };
          } else {
            const payloads = finalizePayloads(activePending);
            const blockInfo = decodeBlockPayload(payloads[0]);
            commitCursor = {
              block_number: blockInfo.number + 1n,
              segment: 0,
              byte_offset: 0,
            };
          }
          const resultCommit = await commitPending({
            config,
            db,
            response,
            previousCursor: streamCursor,
            cursor: commitCursor,
            headNumber,
            pending: activePending,
            lastSizeDay,
          });
          if (resultCommit) {
            lastSizeDay = resultCommit.lastSizeDay;
            streamCursor = commitCursor;
          }
          activePending = null;
          const hasNextChunk = i + 1 < response.chunks.length;
          if (hasNextChunk) {
            activePending = newPendingFromChunk(response.chunks[i + 1]);
          }
          continue;
        }
        if (streamCursor) {
          streamCursor = {
            block_number: streamCursor.block_number,
            segment: activePending.segment,
            byte_offset: activePending.offset,
          };
        }
      }
      pending = activePending;

      cursor = finalCursor;
      if (!streamCursor) {
        logFatal(
          config.chainId,
          "InvalidCursor",
          previousCursor,
          headNumber,
          response,
          null,
          "unable to establish consumed stream cursor without initial cursor"
        );
        process.exit(1);
      }
      if (
        streamCursor.block_number !== finalCursor.block_number ||
        streamCursor.segment !== finalCursor.segment ||
        streamCursor.byte_offset !== finalCursor.byte_offset
      ) {
        // next_cursorの正当性は「chunk streamの実消費結果と一致するか」で最終保証する。
        // これにより、複数block同梱レスポンス(+N進行)も欠落なく安全に許容できる。
        logFatal(
          config.chainId,
          "InvalidCursor",
          previousCursor,
          headNumber,
          response,
          null,
          "response next_cursor does not match consumed chunk stream"
        );
        process.exit(1);
      }
      }
    }
  } finally {
    teardownSignals();
    teardownFatalHandlers();
    await db.close();
  }
}

async function bootstrapCursorFromMissingData(
  client: { getPruneStatus: () => Promise<PruneStatusView> },
  headNumber: bigint,
  chainId: string
): Promise<Cursor> {
  let blockNumber = 1n;
  try {
    const pruneStatus = await client.getPruneStatus();
    if (pruneStatus.pruned_before_block !== null) {
      blockNumber = pruneStatus.pruned_before_block + 1n;
    }
  } catch (err) {
    logWarn(chainId, "bootstrap_prune_status_failed", { err: errMessage(err) });
  }
  if (blockNumber < 1n) {
    blockNumber = 1n;
  }
  if (blockNumber > headNumber) {
    blockNumber = headNumber;
  }
  return { block_number: blockNumber, segment: 0, byte_offset: 0 };
}

function cursorFromPruned(prunedBeforeBlock: bigint, headNumber: bigint): Cursor {
  let blockNumber = prunedBeforeBlock + 1n;
  if (blockNumber < 1n) {
    blockNumber = 1n;
  }
  if (blockNumber > headNumber) {
    blockNumber = headNumber;
  }
  return { block_number: blockNumber, segment: 0, byte_offset: 0 };
}

async function recordPrunedRebase(db: IndexerDb): Promise<void> {
  await db.setMeta("last_error", "Pruned");
  await db.addMetrics(toDayKey(), 0, 0, 0, 1);
}
