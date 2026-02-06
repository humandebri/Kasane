/// <reference path="./globals.d.ts" />
// どこで: indexerメインループ / 何を: pull→検証→DBコミット / なぜ: 仕様のコミット境界を守るため

import { Config, sleep } from "./config";
import { IndexerDb } from "./db";
import { runArchiveGc } from "./archive_gc";
import { Cursor, ExportError, ExportResponse, PruneStatusView, Result } from "./types";
import {
  applyChunks,
  enforceNextCursor,
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
} from "./worker_utils";
import { classifyExportError } from "./worker_errors";
import { commitPending } from "./worker_commit";

export async function runWorkerWithDeps(
  config: Config,
  db: IndexerDb,
  client: {
    getHeadNumber: () => Promise<bigint>;
    exportBlocks: (cursor: Cursor | null, maxBytes: number) => Promise<Result<ExportResponse, ExportError>>;
    getPruneStatus: () => Promise<PruneStatusView>;
  },
  options: { skipGc: boolean }
): Promise<void> {
  if (!options.skipGc) {
    try {
      await runArchiveGc(db, config.archiveDir, config.chainId);
    } catch (err) {
      logWarn(config.chainId, "archive_gc_failed", { message: errMessage(err) });
    }
  }
  let cursor: Cursor | null = db.getCursor();
  let lastHead: bigint | null = null;
  let backoffMs = config.backoffInitialMs;
  let pending: Pending | null = null;
  let retryCount = 0;
  let lastIdleLogAt = 0;
  let stopRequested = false;
  let lastPruneStatusAt = 0;
  let lastSizeDay: number | null = null;
  let lastSqliteBytes = 0;
  let lastArchiveBytes = 0;

  setupSignalHandlers(config.chainId, () => {
    stopRequested = true;
  });
  setupFatalHandlers((err) => {
    logFatal(config.chainId, "Unknown", cursor, lastHead, null, null, "uncaught", err);
    process.exit(1);
  });

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
        db.transaction(() => {
          db.setMeta("prune_status", jsonStringifyBigInt(payload));
          db.setMeta("prune_status_at", String(nowMs));
          db.setMeta("need_prune", status.need_prune ? "1" : "0");
        });
      } catch (err) {
        logWarn(config.chainId, "prune_status_failed", {
          poll_ms: config.pruneStatusPollMs,
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
      const classified = classifyExportError(result.Err, db);
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
          enforceNextCursor(response, cursor);
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

      if (!pending) {
        if (cursor) {
          pending = newPending(cursor);
        } else {
          pending = newPendingFromChunk(response.chunks[0]);
        }
      }

      try {
        applyChunks(pending, response.chunks, cursor);
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
      const previousCursor = cursor;
      cursor = response.next_cursor;

      if (pending.complete) {
        const resultCommit = await commitPending({
          config,
          db,
          response,
          previousCursor,
          cursor,
          headNumber,
          pending,
          lastSizeDay,
        });
        lastSizeDay = resultCommit ? resultCommit.lastSizeDay : lastSizeDay;
        pending = null;
      }
    }
  }

  db.close();
}
