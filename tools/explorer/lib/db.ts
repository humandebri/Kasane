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
  fromAddress: Buffer;
  toAddress: Buffer | null;
  receiptStatus: number | null;
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

export type MetaSnapshot = {
  needPrune: boolean | null;
  pruneStatusRaw: string | null;
  lastHead: bigint | null;
  lastIngestAtMs: bigint | null;
};

export type AddressTxCursor = {
  blockNumber: bigint;
  txIndex: number;
  txHash: Uint8Array;
};

export type AddressTokenTransferCursor = {
  blockNumber: bigint;
  txIndex: number;
  logIndex: number;
  txHash: Uint8Array;
};

export type TokenTransferSummary = {
  txHashHex: string;
  blockNumber: bigint;
  txIndex: number;
  logIndex: number;
  tokenAddress: Buffer;
  fromAddress: Buffer;
  toAddress: Buffer;
  amount: bigint;
};

export type OpsMetricsSample = {
  sampledAtMs: bigint;
  queueLen: bigint;
  cycles: bigint;
  totalSubmitted: bigint;
  totalIncluded: bigint;
  totalDropped: bigint;
  dropCountsJson: string;
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
  const rows = await pool.query<{ tx_hash: Buffer; block_number: string | number; tx_index: number; caller_principal: Buffer | null; from_address: Buffer; to_address: Buffer | null; receipt_status: number | null }>(
    "SELECT tx_hash, block_number, tx_index, caller_principal, from_address, to_address, receipt_status FROM txs ORDER BY block_number DESC, tx_index DESC LIMIT $1",
    [limit]
  );

  return rows.rows.map((row) => ({
    txHashHex: `0x${row.tx_hash.toString("hex")}`,
    blockNumber: BigInt(row.block_number),
    txIndex: row.tx_index,
    callerPrincipal: row.caller_principal ?? null,
    fromAddress: row.from_address,
    toAddress: row.to_address ?? null,
    receiptStatus: row.receipt_status ?? null,
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

  const txRows = await pool.query<{ tx_hash: Buffer; block_number: string | number; tx_index: number; caller_principal: Buffer | null; from_address: Buffer; to_address: Buffer | null; receipt_status: number | null }>(
    "SELECT tx_hash, block_number, tx_index, caller_principal, from_address, to_address, receipt_status FROM txs WHERE block_number = $1 ORDER BY tx_index ASC",
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
      fromAddress: tx.from_address,
      toAddress: tx.to_address ?? null,
      receiptStatus: tx.receipt_status ?? null,
    })),
  };
}

export async function getTx(txHash: Uint8Array): Promise<TxSummary | null> {
  const pool = getPool();
  const row = await pool.query<{ tx_hash: Buffer; block_number: string | number; tx_index: number; caller_principal: Buffer | null; from_address: Buffer; to_address: Buffer | null; receipt_status: number | null }>(
    "SELECT tx_hash, block_number, tx_index, caller_principal, from_address, to_address, receipt_status FROM txs WHERE tx_hash = $1",
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
    fromAddress: hit.from_address,
    toAddress: hit.to_address ?? null,
    receiptStatus: hit.receipt_status ?? null,
  };
}

export async function getTxsByCallerPrincipal(
  callerPrincipal: Uint8Array,
  limit: number
): Promise<TxSummary[]> {
  const pool = getPool();
  const rows = await pool.query<{
    tx_hash: Buffer;
    block_number: string | number;
    tx_index: number;
    caller_principal: Buffer | null;
    from_address: Buffer;
    to_address: Buffer | null;
    receipt_status: number | null;
  }>(
    "SELECT tx_hash, block_number, tx_index, caller_principal, from_address, to_address, receipt_status FROM txs WHERE caller_principal = $1 ORDER BY block_number DESC, tx_index DESC LIMIT $2",
    [Buffer.from(callerPrincipal), limit]
  );
  return rows.rows.map((row) => ({
    txHashHex: `0x${row.tx_hash.toString("hex")}`,
    blockNumber: BigInt(row.block_number),
    txIndex: row.tx_index,
    callerPrincipal: row.caller_principal ?? null,
    fromAddress: row.from_address,
    toAddress: row.to_address ?? null,
    receiptStatus: row.receipt_status ?? null,
  }));
}

export async function getTxsByAddress(
  address: Uint8Array,
  limit: number,
  cursor: AddressTxCursor | null
): Promise<TxSummary[]> {
  const pool = getPool();
  const addressBuf = Buffer.from(address);
  const fetchLimit = limit + 1;
  if (!cursor) {
    const rows = await pool.query<{
      tx_hash: Buffer;
      block_number: string | number;
      tx_index: number;
      caller_principal: Buffer | null;
      from_address: Buffer;
      to_address: Buffer | null;
      receipt_status: number | null;
    }>(
      "SELECT tx_hash, block_number, tx_index, caller_principal, from_address, to_address, receipt_status FROM txs WHERE from_address = $1 OR to_address = $1 ORDER BY block_number DESC, tx_index DESC, tx_hash DESC LIMIT $2",
      [addressBuf, fetchLimit]
    );
    return rows.rows.map((row) => ({
      txHashHex: `0x${row.tx_hash.toString("hex")}`,
      blockNumber: BigInt(row.block_number),
      txIndex: row.tx_index,
      callerPrincipal: row.caller_principal ?? null,
      fromAddress: row.from_address,
      toAddress: row.to_address ?? null,
      receiptStatus: row.receipt_status ?? null,
    }));
  }
  const rows = await pool.query<{
    tx_hash: Buffer;
    block_number: string | number;
    tx_index: number;
    caller_principal: Buffer | null;
    from_address: Buffer;
    to_address: Buffer | null;
    receipt_status: number | null;
  }>(
    "SELECT tx_hash, block_number, tx_index, caller_principal, from_address, to_address, receipt_status FROM txs WHERE (from_address = $1 OR to_address = $1) AND (block_number < $2 OR (block_number = $2 AND tx_index < $3) OR (block_number = $2 AND tx_index = $3 AND tx_hash < $4)) ORDER BY block_number DESC, tx_index DESC, tx_hash DESC LIMIT $5",
    [addressBuf, cursor.blockNumber, cursor.txIndex, Buffer.from(cursor.txHash), fetchLimit]
  );
  return rows.rows.map((row) => ({
    txHashHex: `0x${row.tx_hash.toString("hex")}`,
    blockNumber: BigInt(row.block_number),
    txIndex: row.tx_index,
    callerPrincipal: row.caller_principal ?? null,
    fromAddress: row.from_address,
    toAddress: row.to_address ?? null,
    receiptStatus: row.receipt_status ?? null,
  }));
}

export async function getTokenTransfersByAddress(
  address: Uint8Array,
  limit: number,
  cursor: AddressTokenTransferCursor | null
): Promise<TokenTransferSummary[]> {
  const pool = getPool();
  const addressBuf = Buffer.from(address);
  const fetchLimit = limit + 1;
  if (!cursor) {
    const rows = await pool.query<{
      tx_hash: Buffer;
      block_number: string | number;
      tx_index: number;
      log_index: number;
      token_address: Buffer;
      from_address: Buffer;
      to_address: Buffer;
      amount_numeric: string | number;
    }>(
      "SELECT tx_hash, block_number, tx_index, log_index, token_address, from_address, to_address, amount_numeric FROM token_transfers WHERE from_address = $1 OR to_address = $1 ORDER BY block_number DESC, tx_index DESC, log_index DESC, tx_hash DESC LIMIT $2",
      [addressBuf, fetchLimit]
    );
    return rows.rows.map((row) => ({
      txHashHex: `0x${row.tx_hash.toString("hex")}`,
      blockNumber: BigInt(row.block_number),
      txIndex: row.tx_index,
      logIndex: row.log_index,
      tokenAddress: row.token_address,
      fromAddress: row.from_address,
      toAddress: row.to_address,
      amount: BigInt(row.amount_numeric),
    }));
  }
  const rows = await pool.query<{
    tx_hash: Buffer;
    block_number: string | number;
    tx_index: number;
    log_index: number;
    token_address: Buffer;
    from_address: Buffer;
    to_address: Buffer;
    amount_numeric: string | number;
  }>(
    "SELECT tx_hash, block_number, tx_index, log_index, token_address, from_address, to_address, amount_numeric FROM token_transfers WHERE (from_address = $1 OR to_address = $1) AND (block_number < $2 OR (block_number = $2 AND tx_index < $3) OR (block_number = $2 AND tx_index = $3 AND log_index < $4) OR (block_number = $2 AND tx_index = $3 AND log_index = $4 AND tx_hash < $5)) ORDER BY block_number DESC, tx_index DESC, log_index DESC, tx_hash DESC LIMIT $6",
    [addressBuf, cursor.blockNumber, cursor.txIndex, cursor.logIndex, Buffer.from(cursor.txHash), fetchLimit]
  );
  return rows.rows.map((row) => ({
    txHashHex: `0x${row.tx_hash.toString("hex")}`,
    blockNumber: BigInt(row.block_number),
    txIndex: row.tx_index,
    logIndex: row.log_index,
    tokenAddress: row.token_address,
    fromAddress: row.from_address,
    toAddress: row.to_address,
    amount: BigInt(row.amount_numeric),
  }));
}

export async function getRecentOpsMetricsSamples(limit: number): Promise<OpsMetricsSample[]> {
  const pool = getPool();
  const rows = await pool.query<{
    sampled_at_ms: string | number;
    queue_len: string | number;
    cycles: string | number;
    total_submitted: string | number;
    total_included: string | number;
    total_dropped: string | number;
    drop_counts_json: string;
  }>(
    "SELECT sampled_at_ms, queue_len, cycles, total_submitted, total_included, total_dropped, drop_counts_json FROM ops_metrics_samples ORDER BY sampled_at_ms DESC LIMIT $1",
    [limit]
  );
  return rows.rows.map((row) => ({
    sampledAtMs: BigInt(row.sampled_at_ms),
    queueLen: BigInt(row.queue_len),
    cycles: BigInt(row.cycles),
    totalSubmitted: BigInt(row.total_submitted),
    totalIncluded: BigInt(row.total_included),
    totalDropped: BigInt(row.total_dropped),
    dropCountsJson: row.drop_counts_json,
  }));
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

export async function getMetaSnapshot(): Promise<MetaSnapshot> {
  const pool = getPool();
  const rows = await pool.query<{ key: string; value: string | null }>(
    "select key, value from meta where key in ('need_prune', 'prune_status', 'last_head', 'last_ingest_at')"
  );
  const map = new Map<string, string | null>();
  for (const row of rows.rows) {
    map.set(row.key, row.value);
  }
  const needPruneRaw = map.get("need_prune");
  const needPrune =
    needPruneRaw === undefined || needPruneRaw === null
      ? null
      : needPruneRaw === "1" || needPruneRaw.toLowerCase() === "true";
  return {
    needPrune,
    pruneStatusRaw: map.get("prune_status") ?? null,
    lastHead: toOptionalBigInt(map.get("last_head")),
    lastIngestAtMs: toOptionalBigInt(map.get("last_ingest_at")),
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

function toOptionalBigInt(value: string | null | undefined): bigint | null {
  if (value === undefined || value === null || value.trim() === "") {
    return null;
  }
  try {
    return BigInt(value);
  } catch {
    return null;
  }
}
