/// <reference path="./globals.d.ts" />
// どこで: indexerエントリ / 何を: 依存構築とループ起動 / なぜ: 公開APIを安定させるため

import { Config } from "./config.js";
import { createClient } from "./client.js";
import { IndexerDb } from "./db.js";
import { runWorkerWithDeps as runWorkerWithDepsImpl } from "./worker_loop.js";
import {
  applyChunks,
  enforceNextCursor,
  newPending,
  newPendingFromChunk,
  totalChunkBytes,
} from "./worker_pending.js";
import type {
  Cursor,
  ExportError,
  ExportResponse,
  MemoryBreakdownView,
  MetricsView,
  PruneStatusView,
  Result,
} from "./types.js";

type WorkerClient = {
  getHeadNumber: () => Promise<bigint>;
  exportBlocks: (cursor: Cursor | null, maxBytes: number) => Promise<Result<ExportResponse, ExportError>>;
  getTxMetaByTxId: (txId: Uint8Array) => Promise<{ input: Uint8Array | null; ethTxHash: Uint8Array | null }>;
  getPruneStatus: () => Promise<PruneStatusView>;
  getMetrics: (window: bigint) => Promise<MetricsView>;
  getMemoryBreakdown?: () => Promise<MemoryBreakdownView>;
};

export async function runWorker(config: Config): Promise<void> {
  const db = await IndexerDb.connect({ databaseUrl: config.databaseUrl, poolMax: config.dbPoolMax });
  if (config.retentionEnabled) {
    await db.ensureRetentionSchedule(config.retentionDays);
    if (config.retentionDryRun) {
      await db.runRetentionCleanup(config.retentionDays, true);
    }
  }
  const client = await createClient(config);
  await runWorkerWithDepsImpl(config, db, client, {
    skipGc: false,
    recreateClient: async () => createClient(config),
  });
}

export async function runWorkerWithDeps(
  config: Config,
  db: IndexerDb,
  client: WorkerClient,
  options: { skipGc: boolean; recreateClient?: () => Promise<WorkerClient> }
): Promise<void> {
  await runWorkerWithDepsImpl(config, db, client, options);
}

export const _test = {
  applyChunks,
  enforceNextCursor,
  newPending,
  newPendingFromChunk,
  totalChunkBytes,
};
