/// <reference path="./globals.d.ts" />
// どこで: indexer補助CLI / 何を: eth_tx_hash 欠損行の手動補完 / なぜ: 既存データの404を解消するため

import { Pool } from "pg";
import type { QueryResult } from "pg";
import { createClient } from "./client";
import { loadConfig, sleep } from "./config";

type Row = { tx_hash: Buffer; block_number: bigint; tx_index: number };

const DEFAULT_BATCH_SIZE = 200;
const DEFAULT_MAX_BATCHES = 0;
const DEFAULT_RETRY_MAX = 2;
const DEFAULT_RETRY_SLEEP_MS = 100;

function readPositiveInt(raw: string | undefined, fallback: number): number {
  if (!raw) {
    return fallback;
  }
  const value = Number(raw);
  if (!Number.isFinite(value) || value <= 0 || !Number.isInteger(value)) {
    throw new Error(`invalid positive integer: ${raw}`);
  }
  return value;
}

function readNonNegativeInt(raw: string | undefined, fallback: number): number {
  if (!raw) {
    return fallback;
  }
  const value = Number(raw);
  if (!Number.isFinite(value) || value < 0 || !Number.isInteger(value)) {
    throw new Error(`invalid non-negative integer: ${raw}`);
  }
  return value;
}

async function fetchMetaWithRetry(
  getTxMetaByTxId: (txId: Uint8Array) => Promise<{ input: Uint8Array | null; ethTxHash: Uint8Array | null }>,
  txId: Uint8Array,
  retryMax: number,
  retrySleepMs: number
): Promise<{ input: Uint8Array | null; ethTxHash: Uint8Array | null } | null> {
  for (let attempt = 0; attempt <= retryMax; attempt += 1) {
    try {
      return await getTxMetaByTxId(txId);
    } catch (err) {
      if (attempt >= retryMax) {
        const msg = err instanceof Error ? err.message : String(err);
        process.stderr.write(`[backfill_eth_tx_hash] tx meta fetch failed: ${msg}\n`);
        return null;
      }
      await sleep(retrySleepMs);
    }
  }
  return null;
}

async function main(): Promise<void> {
  const config = loadConfig(process.env);
  const batchSize = readPositiveInt(process.env.INDEXER_BACKFILL_BATCH_SIZE, DEFAULT_BATCH_SIZE);
  const maxBatches = readNonNegativeInt(process.env.INDEXER_BACKFILL_MAX_BATCHES, DEFAULT_MAX_BATCHES);
  const retryMax = readNonNegativeInt(process.env.INDEXER_BACKFILL_RETRY_MAX, DEFAULT_RETRY_MAX);
  const retrySleepMs = readPositiveInt(process.env.INDEXER_BACKFILL_RETRY_SLEEP_MS, DEFAULT_RETRY_SLEEP_MS);
  const client = await createClient(config);
  const pool = new Pool({ connectionString: config.databaseUrl, max: config.dbPoolMax });

  let processed = 0;
  let updated = 0;
  let unresolved = 0;
  let batchCount = 0;
  let cursor: { blockNumber: bigint; txIndex: number; txHash: Buffer } | null = null;

  try {
    for (;;) {
      if (maxBatches > 0 && batchCount >= maxBatches) {
        break;
      }
      const rows: QueryResult<Row> = cursor
        ? await pool.query<Row>(
            "SELECT tx_hash, block_number, tx_index FROM txs WHERE eth_tx_hash IS NULL AND (block_number < $2 OR (block_number = $2 AND tx_index < $3) OR (block_number = $2 AND tx_index = $3 AND tx_hash < $4)) ORDER BY block_number DESC, tx_index DESC, tx_hash DESC LIMIT $1",
            [batchSize, cursor.blockNumber, cursor.txIndex, cursor.txHash]
          )
        : await pool.query<Row>(
            "SELECT tx_hash, block_number, tx_index FROM txs WHERE eth_tx_hash IS NULL ORDER BY block_number DESC, tx_index DESC, tx_hash DESC LIMIT $1",
            [batchSize]
          );
      if ((rows.rowCount ?? 0) === 0) {
        break;
      }
      batchCount += 1;

      for (const row of rows.rows) {
        processed += 1;
        const txId = new Uint8Array(row.tx_hash);
        const meta = await fetchMetaWithRetry(client.getTxMetaByTxId, txId, retryMax, retrySleepMs);
        if (!meta || !meta.ethTxHash || meta.ethTxHash.length === 0) {
          unresolved += 1;
          continue;
        }
        const result = await pool.query(
          "UPDATE txs SET eth_tx_hash = $2 WHERE tx_hash = $1 AND eth_tx_hash IS NULL",
          [row.tx_hash, Buffer.from(meta.ethTxHash)]
        );
        updated += result.rowCount ?? 0;
      }

      const last: Row | undefined = rows.rows[rows.rows.length - 1];
      if (last) {
        cursor = {
          blockNumber: last.block_number,
          txIndex: last.tx_index,
          txHash: last.tx_hash,
        };
      }

      process.stdout.write(
        `[backfill_eth_tx_hash] batch=${batchCount} processed=${processed} updated=${updated} unresolved=${unresolved}\n`
      );
      if ((rows.rowCount ?? 0) < batchSize || !last) {
        break;
      }
    }
  } finally {
    await pool.end();
  }

  process.stdout.write(
    `[backfill_eth_tx_hash] done processed=${processed} updated=${updated} unresolved=${unresolved} batches=${batchCount}\n`
  );
}

main().catch((err) => {
  const detail = err instanceof Error ? err.stack ?? err.message : String(err);
  process.stderr.write(`[backfill_eth_tx_hash] fatal: ${detail}\n`);
  process.exit(1);
});
