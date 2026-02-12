// どこで: Explorer DB層 / 何を: Postgres読み取りクエリを集中管理 / なぜ: UI層と永続化層を分離して保守しやすくするため

import { Pool } from "pg";
import { loadConfig } from "./config";

export type BlockSummary = {
  number: bigint;
  hashHex: string | null;
  timestamp: bigint;
  txCount: number;
};

export type TxSummary = {
  txHashHex: string;
  blockNumber: bigint;
  txIndex: number;
  callerPrincipal: Buffer | null;
};

export type BlockDetails = {
  block: BlockSummary;
  txs: TxSummary[];
};

export type OverviewStats = {
  totalBlocks: bigint;
  totalTxs: bigint;
  latestDay: number | null;
  latestDayBlocks: bigint;
  latestDayRawBytes: bigint;
  latestDayCompressedBytes: bigint;
};

let sharedPool: Pool | null = null;

function getPool(): Pool {
  if (sharedPool) {
    return sharedPool;
  }
  const cfg = loadConfig(process.env);
  sharedPool = new Pool({ connectionString: cfg.databaseUrl, max: cfg.dbPoolMax });
  return sharedPool;
}

export async function getMaxBlockNumber(): Promise<bigint | null> {
  const pool = getPool();
  const row = await pool.query<{ number: string | number | null }>("SELECT MAX(number) as number FROM blocks");
  const raw = row.rows[0]?.number;
  if (raw === null || raw === undefined) {
    return null;
  }
  return BigInt(raw);
}

export async function getLatestBlocks(limit: number): Promise<BlockSummary[]> {
  const pool = getPool();
  const rows = await pool.query<{ number: string | number; hash: Buffer | null; timestamp: string | number; tx_count: number }>(
    "SELECT number, hash, timestamp, tx_count FROM blocks ORDER BY number DESC LIMIT $1",
    [limit]
  );

  return rows.rows.map((row) => ({
    number: BigInt(row.number),
    hashHex: row.hash ? `0x${row.hash.toString("hex")}` : null,
    timestamp: BigInt(row.timestamp),
    txCount: row.tx_count,
  }));
}

export async function getLatestTxs(limit: number): Promise<TxSummary[]> {
  const pool = getPool();
  const rows = await pool.query<{ tx_hash: Buffer; block_number: string | number; tx_index: number; caller_principal: Buffer | null }>(
    "SELECT tx_hash, block_number, tx_index, caller_principal FROM txs ORDER BY block_number DESC, tx_index DESC LIMIT $1",
    [limit]
  );

  return rows.rows.map((row) => ({
    txHashHex: `0x${row.tx_hash.toString("hex")}`,
    blockNumber: BigInt(row.block_number),
    txIndex: row.tx_index,
    callerPrincipal: row.caller_principal ?? null,
  }));
}

export async function getBlockDetails(blockNumber: bigint): Promise<BlockDetails | null> {
  const pool = getPool();
  const blockRow = await pool.query<{ number: string | number; hash: Buffer | null; timestamp: string | number; tx_count: number }>(
    "SELECT number, hash, timestamp, tx_count FROM blocks WHERE number = $1",
    [blockNumber]
  );

  if (blockRow.rowCount === 0) {
    return null;
  }

  const txRows = await pool.query<{ tx_hash: Buffer; block_number: string | number; tx_index: number; caller_principal: Buffer | null }>(
    "SELECT tx_hash, block_number, tx_index, caller_principal FROM txs WHERE block_number = $1 ORDER BY tx_index ASC",
    [blockNumber]
  );

  const row = blockRow.rows[0];
  if (!row) {
    return null;
  }

  return {
    block: {
      number: BigInt(row.number),
      hashHex: row.hash ? `0x${row.hash.toString("hex")}` : null,
      timestamp: BigInt(row.timestamp),
      txCount: row.tx_count,
    },
    txs: txRows.rows.map((tx) => ({
      txHashHex: `0x${tx.tx_hash.toString("hex")}`,
      blockNumber: BigInt(tx.block_number),
      txIndex: tx.tx_index,
      callerPrincipal: tx.caller_principal ?? null,
    })),
  };
}

export async function getTx(txHash: Uint8Array): Promise<TxSummary | null> {
  const pool = getPool();
  const row = await pool.query<{ tx_hash: Buffer; block_number: string | number; tx_index: number; caller_principal: Buffer | null }>(
    "SELECT tx_hash, block_number, tx_index, caller_principal FROM txs WHERE tx_hash = $1",
    [Buffer.from(txHash)]
  );

  if (row.rowCount === 0) {
    return null;
  }
  const hit = row.rows[0];
  if (!hit) {
    return null;
  }
  return {
    txHashHex: `0x${hit.tx_hash.toString("hex")}`,
    blockNumber: BigInt(hit.block_number),
    txIndex: hit.tx_index,
    callerPrincipal: hit.caller_principal ?? null,
  };
}

export async function getOverviewStats(): Promise<OverviewStats> {
  const pool = getPool();
  const [blockCount, txCount, dayMetrics] = await Promise.all([
    pool.query<{ n: string | number }>("select count(*) as n from blocks"),
    pool.query<{ n: string | number }>("select count(*) as n from txs"),
    pool.query<{
      day: number;
      blocks_ingested: string | number;
      raw_bytes: string | number;
      compressed_bytes: string | number;
    }>("select day, blocks_ingested, raw_bytes, compressed_bytes from metrics_daily order by day desc limit 1"),
  ]);

  const m = dayMetrics.rows[0];
  return {
    totalBlocks: BigInt(blockCount.rows[0]?.n ?? 0),
    totalTxs: BigInt(txCount.rows[0]?.n ?? 0),
    latestDay: m?.day ?? null,
    latestDayBlocks: BigInt(m?.blocks_ingested ?? 0),
    latestDayRawBytes: BigInt(m?.raw_bytes ?? 0),
    latestDayCompressedBytes: BigInt(m?.compressed_bytes ?? 0),
  };
}

export async function closeExplorerPool(): Promise<void> {
  if (!sharedPool) {
    return;
  }
  const pool = sharedPool;
  sharedPool = null;
  await pool.end();
}

export function setExplorerPool(pool: Pool): void {
  sharedPool = pool;
}
