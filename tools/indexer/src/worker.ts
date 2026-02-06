/// <reference path="./globals.d.ts" />
// どこで: indexerエントリ / 何を: 依存構築とループ起動 / なぜ: 公開APIを安定させるため

import { Config } from "./config";
import { createClient } from "./client";
import { IndexerDb } from "./db";
import { runWorkerWithDeps as runWorkerWithDepsImpl } from "./worker_loop";
import {
  applyChunks,
  enforceNextCursor,
  newPending,
  newPendingFromChunk,
  totalChunkBytes,
} from "./worker_pending";
import type { Cursor, ExportError, ExportResponse, PruneStatusView, Result } from "./types";

export async function runWorker(config: Config): Promise<void> {
  const db = new IndexerDb(config.dbPath);
  const client = await createClient(config);
  await runWorkerWithDepsImpl(config, db, client, { skipGc: false });
}

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
  await runWorkerWithDepsImpl(config, db, client, options);
}

export const _test = {
  applyChunks,
  enforceNextCursor,
  newPending,
  newPendingFromChunk,
  totalChunkBytes,
};
