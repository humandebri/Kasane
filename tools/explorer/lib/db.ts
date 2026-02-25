// どこで: Explorer DB層 / 何を: Postgres読み取りクエリを集中管理 / なぜ: UI層と永続化層を分離して保守しやすくするため

import { Pool } from "pg";
import { loadConfig } from "./config";

export type BlockSummary = {
  number: bigint;
  hashHex: string | null;
  timestamp: bigint;
  txCount: number;
  gasUsed: bigint | null;
};

export type TxSummary = {
  txHashHex: string;
  blockNumber: bigint;
  blockTimestamp: bigint | null;
  txIndex: number;
  callerPrincipal: Buffer | null;
  fromAddress: Buffer;
  toAddress: Buffer | null;
  createdContractAddress: Buffer | null;
  txSelector: Buffer | null;
  receiptStatus: number | null;
};

export type TxLookup = TxSummary & {
  ethTxHashHex: string | null;
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
  memoryBreakdownRaw: string | null;
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
  blockTimestamp: bigint | null;
  txIndex: number;
  logIndex: number;
  receiptStatus: number | null;
  txSelector: Buffer | null;
  tokenAddress: Buffer;
  fromAddress: Buffer;
  toAddress: Buffer;
  amount: bigint;
};

export type OpsMetricsSample = {
  sampledAtMs: bigint;
  queueLen: bigint;
  cycles: bigint;
  prunedBeforeBlock: bigint | null;
  estimatedKeptBytes: bigint | null;
  lowWaterBytes: bigint | null;
  highWaterBytes: bigint | null;
  hardEmergencyBytes: bigint | null;
  totalSubmitted: bigint;
  totalIncluded: bigint;
  totalDropped: bigint;
  dropCountsJson: string;
};

export type VerifyStatus = "queued" | "running" | "succeeded" | "failed";

export type VerifyRequest = {
  id: string;
  contractAddress: string;
  chainId: number;
  submittedBy: string;
  status: VerifyStatus;
  inputHash: string;
  payloadCompressed: Uint8Array;
  errorCode: string | null;
  errorMessage: string | null;
  startedAt: bigint | null;
  finishedAt: bigint | null;
  attempts: number;
  createdAt: bigint;
  updatedAt: bigint;
  verifiedContractId: string | null;
};

export type VerifiedContract = {
  id: string;
  contractAddress: string;
  chainId: number;
  contractName: string;
  compilerVersion: string;
  optimizerEnabled: boolean;
  optimizerRuns: number;
  evmVersion: string | null;
  creationMatch: boolean;
  runtimeMatch: boolean;
  abiJson: string;
  sourceBlobId: string;
  metadataBlobId: string;
  publishedAt: bigint;
};

export type VerifyMetricsSample = {
  sampledAtMs: bigint;
  queueDepth: bigint;
  successCount: bigint;
  failedCount: bigint;
  avgDurationMs: bigint | null;
  p50DurationMs: bigint | null;
  p95DurationMs: bigint | null;
  failByCodeJson: string;
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
  const rows = await pool.query<{
    number: string | number;
    hash: Buffer | null;
    timestamp: string | number;
    tx_count: number;
    gas_used: string | number | null;
  }>(
    "SELECT number, hash, timestamp, tx_count, gas_used FROM blocks ORDER BY number DESC LIMIT $1",
    [limit]
  );

  return rows.rows.map((row) => ({
    number: BigInt(row.number),
    hashHex: row.hash ? `0x${row.hash.toString("hex")}` : null,
    timestamp: BigInt(row.timestamp),
    txCount: row.tx_count,
    gasUsed: row.gas_used === null ? null : BigInt(row.gas_used),
  }));
}

export async function getLatestBlocksPage(limit: number, offset: number): Promise<BlockSummary[]> {
  const pool = getPool();
  const rows = await pool.query<{
    number: string | number;
    hash: Buffer | null;
    timestamp: string | number;
    tx_count: number;
    gas_used: string | number | null;
  }>(
    "SELECT number, hash, timestamp, tx_count, gas_used FROM blocks ORDER BY number DESC LIMIT $1 OFFSET $2",
    [limit, offset]
  );
  return rows.rows.map((row) => ({
    number: BigInt(row.number),
    hashHex: row.hash ? `0x${row.hash.toString("hex")}` : null,
    timestamp: BigInt(row.timestamp),
    txCount: row.tx_count,
    gasUsed: row.gas_used === null ? null : BigInt(row.gas_used),
  }));
}

export async function getLatestTxs(limit: number): Promise<TxSummary[]> {
  const pool = getPool();
  const rows = await pool.query<{
    tx_hash: Buffer;
    block_number: string | number;
    block_timestamp: string | number | null;
    tx_index: number;
    caller_principal: Buffer | null;
    from_address: Buffer;
    to_address: Buffer | null;
    contract_address: Buffer | null;
    tx_selector: Buffer | null;
    receipt_status: number | null;
  }>(
    "SELECT t.tx_hash, t.block_number, b.timestamp AS block_timestamp, t.tx_index, t.caller_principal, t.from_address, t.to_address, r.contract_address, t.tx_selector, t.receipt_status FROM txs t LEFT JOIN tx_receipts_index r ON r.tx_hash = t.tx_hash LEFT JOIN blocks b ON b.number = t.block_number ORDER BY t.block_number DESC, t.tx_index DESC LIMIT $1",
    [limit]
  );

  return rows.rows.map((row) => ({
    txHashHex: `0x${row.tx_hash.toString("hex")}`,
    blockNumber: BigInt(row.block_number),
    blockTimestamp: row.block_timestamp === null ? null : BigInt(row.block_timestamp),
    txIndex: row.tx_index,
    callerPrincipal: row.caller_principal ?? null,
    fromAddress: row.from_address,
    toAddress: row.to_address ?? null,
    createdContractAddress: row.contract_address ?? null,
    txSelector: row.tx_selector ?? null,
    receiptStatus: row.receipt_status ?? null,
  }));
}

export async function getLatestTxsPage(limit: number, offset: number): Promise<TxSummary[]> {
  const pool = getPool();
  const rows = await pool.query<{
    tx_hash: Buffer;
    block_number: string | number;
    block_timestamp: string | number | null;
    tx_index: number;
    caller_principal: Buffer | null;
    from_address: Buffer;
    to_address: Buffer | null;
    contract_address: Buffer | null;
    tx_selector: Buffer | null;
    receipt_status: number | null;
  }>(
    "SELECT t.tx_hash, t.block_number, b.timestamp AS block_timestamp, t.tx_index, t.caller_principal, t.from_address, t.to_address, r.contract_address, t.tx_selector, t.receipt_status FROM txs t LEFT JOIN tx_receipts_index r ON r.tx_hash = t.tx_hash LEFT JOIN blocks b ON b.number = t.block_number ORDER BY t.block_number DESC, t.tx_index DESC LIMIT $1 OFFSET $2",
    [limit, offset]
  );

  return rows.rows.map((row) => ({
    txHashHex: `0x${row.tx_hash.toString("hex")}`,
    blockNumber: BigInt(row.block_number),
    blockTimestamp: row.block_timestamp === null ? null : BigInt(row.block_timestamp),
    txIndex: row.tx_index,
    callerPrincipal: row.caller_principal ?? null,
    fromAddress: row.from_address,
    toAddress: row.to_address ?? null,
    createdContractAddress: row.contract_address ?? null,
    txSelector: row.tx_selector ?? null,
    receiptStatus: row.receipt_status ?? null,
  }));
}

export async function getLatestTxsPageByBlock(limit: number, offset: number, blockNumber: bigint): Promise<TxSummary[]> {
  const pool = getPool();
  const rows = await pool.query<{
    tx_hash: Buffer;
    block_number: string | number;
    block_timestamp: string | number | null;
    tx_index: number;
    caller_principal: Buffer | null;
    from_address: Buffer;
    to_address: Buffer | null;
    contract_address: Buffer | null;
    tx_selector: Buffer | null;
    receipt_status: number | null;
  }>(
    "SELECT t.tx_hash, t.block_number, b.timestamp AS block_timestamp, t.tx_index, t.caller_principal, t.from_address, t.to_address, r.contract_address, t.tx_selector, t.receipt_status FROM txs t LEFT JOIN tx_receipts_index r ON r.tx_hash = t.tx_hash LEFT JOIN blocks b ON b.number = t.block_number WHERE t.block_number = $3 ORDER BY t.tx_index ASC LIMIT $1 OFFSET $2",
    [limit, offset, blockNumber]
  );

  return rows.rows.map((row) => ({
    txHashHex: `0x${row.tx_hash.toString("hex")}`,
    blockNumber: BigInt(row.block_number),
    blockTimestamp: row.block_timestamp === null ? null : BigInt(row.block_timestamp),
    txIndex: row.tx_index,
    callerPrincipal: row.caller_principal ?? null,
    fromAddress: row.from_address,
    toAddress: row.to_address ?? null,
    createdContractAddress: row.contract_address ?? null,
    txSelector: row.tx_selector ?? null,
    receiptStatus: row.receipt_status ?? null,
  }));
}

export async function getTxCountByBlock(blockNumber: bigint): Promise<bigint> {
  const pool = getPool();
  const rows = await pool.query<{ n: string | number }>("SELECT COUNT(*) AS n FROM txs WHERE block_number = $1", [blockNumber]);
  return BigInt(rows.rows[0]?.n ?? 0);
}

export async function getBlockDetails(blockNumber: bigint): Promise<BlockDetails | null> {
  const pool = getPool();
  const blockRow = await pool.query<{
    number: string | number;
    hash: Buffer | null;
    timestamp: string | number;
    tx_count: number;
    gas_used: string | number | null;
  }>(
    "SELECT number, hash, timestamp, tx_count, gas_used FROM blocks WHERE number = $1",
    [blockNumber]
  );

  if (blockRow.rowCount === 0) {
    return null;
  }

  const txRows = await pool.query<{
    tx_hash: Buffer;
    block_number: string | number;
    block_timestamp: string | number | null;
    tx_index: number;
    caller_principal: Buffer | null;
    from_address: Buffer;
    to_address: Buffer | null;
    contract_address: Buffer | null;
    tx_selector: Buffer | null;
    receipt_status: number | null;
  }>(
    "SELECT t.tx_hash, t.block_number, b.timestamp AS block_timestamp, t.tx_index, t.caller_principal, t.from_address, t.to_address, r.contract_address, t.tx_selector, t.receipt_status FROM txs t LEFT JOIN tx_receipts_index r ON r.tx_hash = t.tx_hash LEFT JOIN blocks b ON b.number = t.block_number WHERE t.block_number = $1 ORDER BY t.tx_index ASC",
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
      gasUsed: row.gas_used === null ? null : BigInt(row.gas_used),
    },
    txs: txRows.rows.map((tx) => ({
      txHashHex: `0x${tx.tx_hash.toString("hex")}`,
      blockNumber: BigInt(tx.block_number),
      blockTimestamp: tx.block_timestamp === null ? null : BigInt(tx.block_timestamp),
      txIndex: tx.tx_index,
      callerPrincipal: tx.caller_principal ?? null,
      fromAddress: tx.from_address,
      toAddress: tx.to_address ?? null,
      createdContractAddress: tx.contract_address ?? null,
      txSelector: tx.tx_selector ?? null,
      receiptStatus: tx.receipt_status ?? null,
    })),
  };
}

export async function getTx(txHash: Uint8Array): Promise<TxSummary | null> {
  const pool = getPool();
  const row = await pool.query<{
    tx_hash: Buffer;
    block_number: string | number;
    block_timestamp: string | number | null;
    tx_index: number;
    caller_principal: Buffer | null;
    from_address: Buffer;
    to_address: Buffer | null;
    contract_address: Buffer | null;
    tx_selector: Buffer | null;
    receipt_status: number | null;
  }>(
    "SELECT t.tx_hash, t.block_number, b.timestamp AS block_timestamp, t.tx_index, t.caller_principal, t.from_address, t.to_address, r.contract_address, t.tx_selector, t.receipt_status FROM txs t LEFT JOIN tx_receipts_index r ON r.tx_hash = t.tx_hash LEFT JOIN blocks b ON b.number = t.block_number WHERE t.tx_hash = $1",
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
    blockTimestamp: hit.block_timestamp === null ? null : BigInt(hit.block_timestamp),
    txIndex: hit.tx_index,
    callerPrincipal: hit.caller_principal ?? null,
    fromAddress: hit.from_address,
    toAddress: hit.to_address ?? null,
    createdContractAddress: hit.contract_address ?? null,
    txSelector: hit.tx_selector ?? null,
    receiptStatus: hit.receipt_status ?? null,
  };
}

export async function getTxByHashOrEthHash(hash: Uint8Array): Promise<TxLookup | null> {
  const pool = getPool();
  const row = await pool.query<{
    tx_hash: Buffer;
    eth_tx_hash: Buffer | null;
    block_number: string | number;
    block_timestamp: string | number | null;
    tx_index: number;
    caller_principal: Buffer | null;
    from_address: Buffer;
    to_address: Buffer | null;
    contract_address: Buffer | null;
    tx_selector: Buffer | null;
    receipt_status: number | null;
  }>(
    "SELECT t.tx_hash, t.eth_tx_hash, t.block_number, b.timestamp AS block_timestamp, t.tx_index, t.caller_principal, t.from_address, t.to_address, r.contract_address, t.tx_selector, t.receipt_status FROM txs t LEFT JOIN tx_receipts_index r ON r.tx_hash = t.tx_hash LEFT JOIN blocks b ON b.number = t.block_number WHERE t.tx_hash = $1 OR t.eth_tx_hash = $1 ORDER BY t.block_number DESC, t.tx_index DESC LIMIT 1",
    [Buffer.from(hash)]
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
    ethTxHashHex: hit.eth_tx_hash ? `0x${hit.eth_tx_hash.toString("hex")}` : null,
    blockNumber: BigInt(hit.block_number),
    blockTimestamp: hit.block_timestamp === null ? null : BigInt(hit.block_timestamp),
    txIndex: hit.tx_index,
    callerPrincipal: hit.caller_principal ?? null,
    fromAddress: hit.from_address,
    toAddress: hit.to_address ?? null,
    createdContractAddress: hit.contract_address ?? null,
    txSelector: hit.tx_selector ?? null,
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
    block_timestamp: string | number | null;
    tx_index: number;
    caller_principal: Buffer | null;
    from_address: Buffer;
    to_address: Buffer | null;
    contract_address: Buffer | null;
    tx_selector: Buffer | null;
    receipt_status: number | null;
  }>(
    "SELECT t.tx_hash, t.block_number, b.timestamp AS block_timestamp, t.tx_index, t.caller_principal, t.from_address, t.to_address, r.contract_address, t.tx_selector, t.receipt_status FROM txs t LEFT JOIN tx_receipts_index r ON r.tx_hash = t.tx_hash LEFT JOIN blocks b ON b.number = t.block_number WHERE t.caller_principal = $1 ORDER BY t.block_number DESC, t.tx_index DESC LIMIT $2",
    [Buffer.from(callerPrincipal), limit]
  );
  return rows.rows.map((row) => ({
    txHashHex: `0x${row.tx_hash.toString("hex")}`,
    blockNumber: BigInt(row.block_number),
    blockTimestamp: row.block_timestamp === null ? null : BigInt(row.block_timestamp),
    txIndex: row.tx_index,
    callerPrincipal: row.caller_principal ?? null,
    fromAddress: row.from_address,
    toAddress: row.to_address ?? null,
    createdContractAddress: row.contract_address ?? null,
    txSelector: row.tx_selector ?? null,
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
      block_timestamp: string | number | null;
      tx_index: number;
      caller_principal: Buffer | null;
      from_address: Buffer;
      to_address: Buffer | null;
      contract_address: Buffer | null;
      tx_selector: Buffer | null;
      receipt_status: number | null;
    }>(
      "SELECT t.tx_hash, t.block_number, b.timestamp AS block_timestamp, t.tx_index, t.caller_principal, t.from_address, t.to_address, r.contract_address, t.tx_selector, t.receipt_status FROM txs t LEFT JOIN tx_receipts_index r ON r.tx_hash = t.tx_hash LEFT JOIN blocks b ON b.number = t.block_number WHERE t.from_address = $1 OR t.to_address = $1 ORDER BY t.block_number DESC, t.tx_index DESC, t.tx_hash DESC LIMIT $2",
      [addressBuf, fetchLimit]
    );
    return rows.rows.map((row) => ({
      txHashHex: `0x${row.tx_hash.toString("hex")}`,
      blockNumber: BigInt(row.block_number),
      blockTimestamp: row.block_timestamp === null ? null : BigInt(row.block_timestamp),
      txIndex: row.tx_index,
      callerPrincipal: row.caller_principal ?? null,
      fromAddress: row.from_address,
      toAddress: row.to_address ?? null,
      createdContractAddress: row.contract_address ?? null,
      txSelector: row.tx_selector ?? null,
      receiptStatus: row.receipt_status ?? null,
    }));
  }
  const rows = await pool.query<{
    tx_hash: Buffer;
    block_number: string | number;
    block_timestamp: string | number | null;
    tx_index: number;
    caller_principal: Buffer | null;
    from_address: Buffer;
    to_address: Buffer | null;
    contract_address: Buffer | null;
    tx_selector: Buffer | null;
    receipt_status: number | null;
  }>(
    "SELECT t.tx_hash, t.block_number, b.timestamp AS block_timestamp, t.tx_index, t.caller_principal, t.from_address, t.to_address, r.contract_address, t.tx_selector, t.receipt_status FROM txs t LEFT JOIN tx_receipts_index r ON r.tx_hash = t.tx_hash LEFT JOIN blocks b ON b.number = t.block_number WHERE (t.from_address = $1 OR t.to_address = $1) AND (t.block_number < $2 OR (t.block_number = $2 AND t.tx_index < $3) OR (t.block_number = $2 AND t.tx_index = $3 AND t.tx_hash < $4)) ORDER BY t.block_number DESC, t.tx_index DESC, t.tx_hash DESC LIMIT $5",
    [addressBuf, cursor.blockNumber, cursor.txIndex, Buffer.from(cursor.txHash), fetchLimit]
  );
  return rows.rows.map((row) => ({
    txHashHex: `0x${row.tx_hash.toString("hex")}`,
    blockNumber: BigInt(row.block_number),
    blockTimestamp: row.block_timestamp === null ? null : BigInt(row.block_timestamp),
    txIndex: row.tx_index,
    callerPrincipal: row.caller_principal ?? null,
    fromAddress: row.from_address,
    toAddress: row.to_address ?? null,
    createdContractAddress: row.contract_address ?? null,
    txSelector: row.tx_selector ?? null,
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
      block_timestamp: string | number | null;
      tx_index: number;
      log_index: number;
      receipt_status: number | null;
      tx_selector: Buffer | null;
      token_address: Buffer;
      from_address: Buffer;
      to_address: Buffer;
      amount_numeric: string | number;
    }>(
      "SELECT tt.tx_hash, tt.block_number, b.timestamp AS block_timestamp, tt.tx_index, tt.log_index, t.receipt_status, t.tx_selector, tt.token_address, tt.from_address, tt.to_address, tt.amount_numeric FROM token_transfers tt LEFT JOIN txs t ON t.tx_hash = tt.tx_hash LEFT JOIN blocks b ON b.number = tt.block_number WHERE tt.from_address = $1 OR tt.to_address = $1 ORDER BY tt.block_number DESC, tt.tx_index DESC, tt.log_index DESC, tt.tx_hash DESC LIMIT $2",
      [addressBuf, fetchLimit]
    );
    return rows.rows.map((row) => ({
      txHashHex: `0x${row.tx_hash.toString("hex")}`,
      blockNumber: BigInt(row.block_number),
      blockTimestamp: row.block_timestamp === null ? null : BigInt(row.block_timestamp),
      txIndex: row.tx_index,
      logIndex: row.log_index,
      receiptStatus: row.receipt_status ?? null,
      txSelector: row.tx_selector ?? null,
      tokenAddress: row.token_address,
      fromAddress: row.from_address,
      toAddress: row.to_address,
      amount: BigInt(row.amount_numeric),
    }));
  }
  const rows = await pool.query<{
    tx_hash: Buffer;
    block_number: string | number;
    block_timestamp: string | number | null;
    tx_index: number;
    log_index: number;
    receipt_status: number | null;
    tx_selector: Buffer | null;
    token_address: Buffer;
    from_address: Buffer;
    to_address: Buffer;
    amount_numeric: string | number;
  }>(
    "SELECT tt.tx_hash, tt.block_number, b.timestamp AS block_timestamp, tt.tx_index, tt.log_index, t.receipt_status, t.tx_selector, tt.token_address, tt.from_address, tt.to_address, tt.amount_numeric FROM token_transfers tt LEFT JOIN txs t ON t.tx_hash = tt.tx_hash LEFT JOIN blocks b ON b.number = tt.block_number WHERE (tt.from_address = $1 OR tt.to_address = $1) AND (tt.block_number < $2 OR (tt.block_number = $2 AND tt.tx_index < $3) OR (tt.block_number = $2 AND tt.tx_index = $3 AND tt.log_index < $4) OR (tt.block_number = $2 AND tt.tx_index = $3 AND tt.log_index = $4 AND tt.tx_hash < $5)) ORDER BY tt.block_number DESC, tt.tx_index DESC, tt.log_index DESC, tt.tx_hash DESC LIMIT $6",
    [addressBuf, cursor.blockNumber, cursor.txIndex, cursor.logIndex, Buffer.from(cursor.txHash), fetchLimit]
  );
  return rows.rows.map((row) => ({
    txHashHex: `0x${row.tx_hash.toString("hex")}`,
    blockNumber: BigInt(row.block_number),
    blockTimestamp: row.block_timestamp === null ? null : BigInt(row.block_timestamp),
    txIndex: row.tx_index,
    logIndex: row.log_index,
    receiptStatus: row.receipt_status ?? null,
    txSelector: row.tx_selector ?? null,
    tokenAddress: row.token_address,
    fromAddress: row.from_address,
    toAddress: row.to_address,
    amount: BigInt(row.amount_numeric),
  }));
}

export async function getReceiptStatusByTxHashes(txHashes: string[]): Promise<Map<string, number | null>> {
  const normalized = txHashes
    .map((value) => value.trim().toLowerCase())
    .filter((value) => /^0x[0-9a-f]{64}$/.test(value));
  const unique = [...new Set(normalized)];
  if (unique.length === 0) {
    return new Map();
  }
  const pool = getPool();
  const rows = await pool.query<{ tx_hash: Buffer; receipt_status: number | null }>(
    "SELECT tx_hash, receipt_status FROM txs WHERE tx_hash = ANY($1::bytea[])",
    [unique.map((hashHex) => Buffer.from(hashHex.slice(2), "hex"))]
  );
  const out = new Map<string, number | null>();
  for (const row of rows.rows) {
    out.set(`0x${row.tx_hash.toString("hex")}`, row.receipt_status ?? null);
  }
  return out;
}

export async function getRecentOpsMetricsSamples(limit: number): Promise<OpsMetricsSample[]> {
  const pool = getPool();
  const rows = await pool.query<{
    sampled_at_ms: string | number;
    queue_len: string | number;
    cycles: string | number;
    pruned_before_block: string | number | null;
    estimated_kept_bytes: string | number | null;
    low_water_bytes: string | number | null;
    high_water_bytes: string | number | null;
    hard_emergency_bytes: string | number | null;
    total_submitted: string | number;
    total_included: string | number;
    total_dropped: string | number;
    drop_counts_json: string;
  }>(
    "SELECT sampled_at_ms, queue_len, cycles, pruned_before_block, estimated_kept_bytes, low_water_bytes, high_water_bytes, hard_emergency_bytes, total_submitted, total_included, total_dropped, drop_counts_json FROM ops_metrics_samples ORDER BY sampled_at_ms DESC LIMIT $1",
    [limit]
  );
  return rows.rows.map((row) => ({
    sampledAtMs: BigInt(row.sampled_at_ms),
    queueLen: BigInt(row.queue_len),
    cycles: BigInt(row.cycles),
    prunedBeforeBlock: row.pruned_before_block === null ? null : BigInt(row.pruned_before_block),
    estimatedKeptBytes: row.estimated_kept_bytes === null ? null : BigInt(row.estimated_kept_bytes),
    lowWaterBytes: row.low_water_bytes === null ? null : BigInt(row.low_water_bytes),
    highWaterBytes: row.high_water_bytes === null ? null : BigInt(row.high_water_bytes),
    hardEmergencyBytes: row.hard_emergency_bytes === null ? null : BigInt(row.hard_emergency_bytes),
    totalSubmitted: BigInt(row.total_submitted),
    totalIncluded: BigInt(row.total_included),
    totalDropped: BigInt(row.total_dropped),
    dropCountsJson: row.drop_counts_json,
  }));
}

export async function getOpsMetricsSamplesSince(sinceMs: bigint): Promise<OpsMetricsSample[]> {
  const pool = getPool();
  const rows = await pool.query<{
    sampled_at_ms: string | number;
    queue_len: string | number;
    cycles: string | number;
    pruned_before_block: string | number | null;
    estimated_kept_bytes: string | number | null;
    low_water_bytes: string | number | null;
    high_water_bytes: string | number | null;
    hard_emergency_bytes: string | number | null;
    total_submitted: string | number;
    total_included: string | number;
    total_dropped: string | number;
    drop_counts_json: string;
  }>(
    "SELECT sampled_at_ms, queue_len, cycles, pruned_before_block, estimated_kept_bytes, low_water_bytes, high_water_bytes, hard_emergency_bytes, total_submitted, total_included, total_dropped, drop_counts_json FROM ops_metrics_samples WHERE sampled_at_ms >= $1 ORDER BY sampled_at_ms DESC",
    [sinceMs.toString()]
  );
  return rows.rows.map((row) => ({
    sampledAtMs: BigInt(row.sampled_at_ms),
    queueLen: BigInt(row.queue_len),
    cycles: BigInt(row.cycles),
    prunedBeforeBlock: row.pruned_before_block === null ? null : BigInt(row.pruned_before_block),
    estimatedKeptBytes: row.estimated_kept_bytes === null ? null : BigInt(row.estimated_kept_bytes),
    lowWaterBytes: row.low_water_bytes === null ? null : BigInt(row.low_water_bytes),
    highWaterBytes: row.high_water_bytes === null ? null : BigInt(row.high_water_bytes),
    hardEmergencyBytes: row.hard_emergency_bytes === null ? null : BigInt(row.hard_emergency_bytes),
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
    "select key, value from meta where key in ('need_prune', 'prune_status', 'memory_breakdown', 'last_head', 'last_ingest_at')"
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
    memoryBreakdownRaw: map.get("memory_breakdown") ?? null,
    lastHead: toOptionalBigInt(map.get("last_head")),
    lastIngestAtMs: toOptionalBigInt(map.get("last_ingest_at")),
  };
}

export async function countVerifyRequestsByUserSince(submittedBy: string, sinceMs: bigint): Promise<number> {
  const pool = getPool();
  const out = await pool.query<{ n: string | number }>(
    "SELECT COUNT(*)::bigint AS n FROM verify_requests WHERE submitted_by = $1 AND created_at >= $2",
    [submittedBy, sinceMs]
  );
  return Number(out.rows[0]?.n ?? 0);
}

export async function getVerifyRequestBySubmittedUserAndInputHash(
  submittedBy: string,
  inputHash: string
): Promise<VerifyRequest | null> {
  const pool = getPool();
  const out = await pool.query<{
    id: string;
    contract_address: string;
    chain_id: number;
    submitted_by: string;
    status: VerifyStatus;
    input_hash: string;
    payload_compressed: Buffer;
    error_code: string | null;
    error_message: string | null;
    started_at: string | number | null;
    finished_at: string | number | null;
    attempts: number;
    created_at: string | number;
    updated_at: string | number;
    verified_contract_id: string | null;
  }>(
    "SELECT id, contract_address, chain_id, submitted_by, status, input_hash, payload_compressed, error_code, error_message, started_at, finished_at, attempts, created_at, updated_at, verified_contract_id FROM verify_requests WHERE submitted_by = $1 AND input_hash = $2 LIMIT 1",
    [submittedBy, inputHash]
  );
  if (out.rowCount === 0) {
    return null;
  }
  const row = out.rows[0];
  if (!row) {
    return null;
  }
  return mapVerifyRequestRow(row);
}

export async function insertVerifyRequest(input: {
  id: string;
  contractAddress: string;
  chainId: number;
  submittedBy: string;
  status: VerifyStatus;
  inputHash: string;
  payloadCompressed: Uint8Array;
  createdAtMs: bigint;
}): Promise<void> {
  const pool = getPool();
  await pool.query(
    "INSERT INTO verify_requests(id, contract_address, chain_id, submitted_by, status, input_hash, payload_compressed, attempts, created_at, updated_at) VALUES($1, $2, $3, $4, $5, $6, $7, 0, $8, $8)",
    [
      input.id,
      input.contractAddress,
      input.chainId,
      input.submittedBy,
      input.status,
      input.inputHash,
      Buffer.from(input.payloadCompressed),
      input.createdAtMs.toString(),
    ]
  );
}

export async function getVerifyRequestById(id: string): Promise<VerifyRequest | null> {
  const pool = getPool();
  const out = await pool.query<{
    id: string;
    contract_address: string;
    chain_id: number;
    submitted_by: string;
    status: VerifyStatus;
    input_hash: string;
    payload_compressed: Buffer;
    error_code: string | null;
    error_message: string | null;
    started_at: string | number | null;
    finished_at: string | number | null;
    attempts: number;
    created_at: string | number;
    updated_at: string | number;
    verified_contract_id: string | null;
  }>(
    "SELECT id, contract_address, chain_id, submitted_by, status, input_hash, payload_compressed, error_code, error_message, started_at, finished_at, attempts, created_at, updated_at, verified_contract_id FROM verify_requests WHERE id = $1 LIMIT 1",
    [id]
  );
  if (out.rowCount === 0) {
    return null;
  }
  const row = out.rows[0];
  if (!row) {
    return null;
  }
  return mapVerifyRequestRow(row);
}

export async function claimNextVerifyRequest(nowMs: bigint): Promise<VerifyRequest | null> {
  const pool = getPool();
  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    const lockRow = await client.query<{ id: string }>(
      "SELECT id FROM verify_requests WHERE status = 'queued' ORDER BY created_at ASC LIMIT 1 FOR UPDATE SKIP LOCKED"
    );
    const id = lockRow.rows[0]?.id;
    if (!id) {
      await client.query("COMMIT");
      return null;
    }
    await client.query(
      "UPDATE verify_requests SET status = 'running', attempts = attempts + 1, started_at = $2, updated_at = $2 WHERE id = $1",
      [id, nowMs.toString()]
    );
    const out = await client.query<{
      id: string;
      contract_address: string;
      chain_id: number;
      submitted_by: string;
      status: VerifyStatus;
      input_hash: string;
      payload_compressed: Buffer;
      error_code: string | null;
      error_message: string | null;
      started_at: string | number | null;
      finished_at: string | number | null;
      attempts: number;
      created_at: string | number;
      updated_at: string | number;
      verified_contract_id: string | null;
    }>(
      "SELECT id, contract_address, chain_id, submitted_by, status, input_hash, payload_compressed, error_code, error_message, started_at, finished_at, attempts, created_at, updated_at, verified_contract_id FROM verify_requests WHERE id = $1 LIMIT 1",
      [id]
    );
    await client.query("COMMIT");
    if (out.rowCount === 0) {
      return null;
    }
    const mapped = out.rows[0];
    if (!mapped) {
      return null;
    }
    return mapVerifyRequestRow(mapped);
  } catch (err) {
    await client.query("ROLLBACK");
    throw err;
  } finally {
    client.release();
  }
}

export async function appendVerifyJobLog(input: {
  id: string;
  requestId: string;
  level: "info" | "warn" | "error";
  message: string;
  createdAtMs: bigint;
  submittedBy?: string | null;
  ipHash?: string | null;
  uaHash?: string | null;
  eventType?: "submit" | "start" | "success" | "fail" | "retry";
}): Promise<void> {
  const pool = getPool();
  await pool.query(
    "INSERT INTO verify_job_logs(id, request_id, level, message, created_at, submitted_by, ip_hash, ua_hash, event_type) VALUES($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    [
      input.id,
      input.requestId,
      input.level,
      input.message,
      input.createdAtMs.toString(),
      input.submittedBy ?? null,
      input.ipHash ?? null,
      input.uaHash ?? null,
      input.eventType ?? null,
    ]
  );
}

export async function upsertVerifyBlob(input: {
  id: string;
  sha256: string;
  encoding: "gzip";
  rawSize: number;
  blob: Uint8Array;
}): Promise<string> {
  const pool = getPool();
  await pool.query(
    "INSERT INTO verify_blobs(id, sha256, encoding, raw_size, blob) VALUES($1, $2, $3, $4, $5) ON CONFLICT (sha256) DO NOTHING",
    [input.id, input.sha256, input.encoding, input.rawSize, Buffer.from(input.blob)]
  );
  const out = await pool.query<{ id: string }>("SELECT id FROM verify_blobs WHERE sha256 = $1 LIMIT 1", [input.sha256]);
  const id = out.rows[0]?.id;
  if (!id) {
    throw new Error("failed to persist verify blob");
  }
  return id;
}

export async function upsertVerifiedContract(input: {
  id: string;
  contractAddress: string;
  chainId: number;
  contractName: string;
  compilerVersion: string;
  optimizerEnabled: boolean;
  optimizerRuns: number;
  evmVersion: string | null;
  creationMatch: boolean;
  runtimeMatch: boolean;
  abiJson: string;
  sourceBlobId: string;
  metadataBlobId: string;
  publishedAtMs: bigint;
}): Promise<string> {
  const pool = getPool();
  const out = await pool.query<{ id: string }>(
    "INSERT INTO verified_contracts(id, contract_address, chain_id, contract_name, compiler_version, optimizer_enabled, optimizer_runs, evm_version, creation_match, runtime_match, abi_json, source_blob_id, metadata_blob_id, published_at) VALUES($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) ON CONFLICT (contract_address, chain_id) DO UPDATE SET contract_name = EXCLUDED.contract_name, compiler_version = EXCLUDED.compiler_version, optimizer_enabled = EXCLUDED.optimizer_enabled, optimizer_runs = EXCLUDED.optimizer_runs, evm_version = EXCLUDED.evm_version, creation_match = EXCLUDED.creation_match, runtime_match = EXCLUDED.runtime_match, abi_json = EXCLUDED.abi_json, source_blob_id = EXCLUDED.source_blob_id, metadata_blob_id = EXCLUDED.metadata_blob_id, published_at = EXCLUDED.published_at RETURNING id",
    [
      input.id,
      input.contractAddress,
      input.chainId,
      input.contractName,
      input.compilerVersion,
      input.optimizerEnabled,
      input.optimizerRuns,
      input.evmVersion,
      input.creationMatch,
      input.runtimeMatch,
      input.abiJson,
      input.sourceBlobId,
      input.metadataBlobId,
      input.publishedAtMs.toString(),
    ]
  );
  const id = out.rows[0]?.id;
  if (!id) {
    throw new Error("failed to upsert verified contract");
  }
  return id;
}

export async function markVerifyRequestSucceeded(input: {
  id: string;
  verifiedContractId: string;
  finishedAtMs: bigint;
}): Promise<void> {
  const pool = getPool();
  await pool.query(
    "UPDATE verify_requests SET status = 'succeeded', error_code = NULL, error_message = NULL, finished_at = $2, updated_at = $2, verified_contract_id = $3 WHERE id = $1",
    [input.id, input.finishedAtMs.toString(), input.verifiedContractId]
  );
}

export async function markVerifyRequestFailed(input: {
  id: string;
  errorCode: string;
  errorMessage: string;
  finishedAtMs: bigint;
}): Promise<void> {
  const pool = getPool();
  await pool.query(
    "UPDATE verify_requests SET status = 'failed', error_code = $2, error_message = $3, finished_at = $4, updated_at = $4 WHERE id = $1",
    [input.id, input.errorCode, input.errorMessage, input.finishedAtMs.toString()]
  );
}

export async function requeueVerifyRequest(input: {
  id: string;
  errorCode: string;
  errorMessage: string;
  updatedAtMs: bigint;
}): Promise<void> {
  const pool = getPool();
  await pool.query(
    "UPDATE verify_requests SET status = 'queued', error_code = $2, error_message = $3, updated_at = $4 WHERE id = $1",
    [input.id, input.errorCode, input.errorMessage, input.updatedAtMs.toString()]
  );
}

export async function deleteVerifyLogsOlderThan(cutoffMs: bigint): Promise<number> {
  const pool = getPool();
  const out = await pool.query("DELETE FROM verify_job_logs WHERE created_at < $1", [cutoffMs.toString()]);
  return out.rowCount ?? 0;
}

export async function getVerifiedContractByAddress(
  contractAddress: string,
  chainId: number
): Promise<VerifiedContract | null> {
  const pool = getPool();
  const out = await pool.query<{
    id: string;
    contract_address: string;
    chain_id: number;
    contract_name: string;
    compiler_version: string;
    optimizer_enabled: boolean;
    optimizer_runs: number;
    evm_version: string | null;
    creation_match: boolean;
    runtime_match: boolean;
    abi_json: string;
    source_blob_id: string;
    metadata_blob_id: string;
    published_at: string | number;
  }>(
    "SELECT id, contract_address, chain_id, contract_name, compiler_version, optimizer_enabled, optimizer_runs, evm_version, creation_match, runtime_match, abi_json, source_blob_id, metadata_blob_id, published_at FROM verified_contracts WHERE contract_address = $1 AND chain_id = $2 LIMIT 1",
    [contractAddress.toLowerCase(), chainId]
  );
  if (out.rowCount === 0) {
    return null;
  }
  const row = out.rows[0];
  if (!row) {
    return null;
  }
  return {
    id: row.id,
    contractAddress: row.contract_address,
    chainId: row.chain_id,
    contractName: row.contract_name,
    compilerVersion: row.compiler_version,
    optimizerEnabled: row.optimizer_enabled,
    optimizerRuns: row.optimizer_runs,
    evmVersion: row.evm_version,
    creationMatch: row.creation_match,
    runtimeMatch: row.runtime_match,
    abiJson: row.abi_json,
    sourceBlobId: row.source_blob_id,
    metadataBlobId: row.metadata_blob_id,
    publishedAt: BigInt(row.published_at),
  };
}

export async function getVerifyBlobById(id: string): Promise<{ encoding: string; rawSize: number; blob: Uint8Array } | null> {
  const pool = getPool();
  const out = await pool.query<{ encoding: string; raw_size: number; blob: Buffer }>(
    "SELECT encoding, raw_size, blob FROM verify_blobs WHERE id = $1 LIMIT 1",
    [id]
  );
  if (out.rowCount === 0) {
    return null;
  }
  const row = out.rows[0];
  if (!row) {
    return null;
  }
  return {
    encoding: row.encoding,
    rawSize: row.raw_size,
    blob: new Uint8Array(row.blob),
  };
}

export async function getDeployTxInputByContractAddress(contractAddress: Uint8Array): Promise<{
  found: boolean;
  txInput: Uint8Array | null;
}> {
  const pool = getPool();
  const out = await pool.query<{ tx_input: Buffer | null }>(
    "SELECT t.tx_input FROM tx_receipts_index r JOIN txs t ON t.tx_hash = r.tx_hash WHERE r.contract_address = $1 ORDER BY r.block_number DESC, r.tx_index DESC LIMIT 1",
    [Buffer.from(contractAddress)]
  );
  if (out.rowCount === 0) {
    return { found: false, txInput: null };
  }
  const row = out.rows[0];
  if (!row || !row.tx_input) {
    return { found: true, txInput: null };
  }
  return { found: true, txInput: new Uint8Array(row.tx_input) };
}

export async function consumeVerifyReplayJti(params: {
  jti: string;
  sub: string;
  scope: string;
  expSec: bigint;
  consumedAtMs: bigint;
}): Promise<boolean> {
  const pool = getPool();
  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    const nowSec = BigInt(Math.floor(Date.now() / 1000));
    await client.query("DELETE FROM verify_auth_replay WHERE exp < $1", [nowSec.toString()]);
    try {
      await client.query(
        "INSERT INTO verify_auth_replay(jti, sub, scope, exp, consumed_at) VALUES($1, $2, $3, $4, $5)",
        [params.jti, params.sub, params.scope, params.expSec.toString(), params.consumedAtMs.toString()]
      );
    } catch (err) {
      if (isPgUniqueViolation(err)) {
        await client.query("COMMIT");
        return false;
      }
      throw err;
    }
    await client.query("COMMIT");
    return true;
  } catch (err) {
    await client.query("ROLLBACK");
    throw err;
  } finally {
    client.release();
  }
}

export async function deleteVerifyReplayExpired(nowSec: bigint): Promise<number> {
  const pool = getPool();
  const out = await pool.query("DELETE FROM verify_auth_replay WHERE exp < $1", [nowSec.toString()]);
  return out.rowCount ?? 0;
}

export async function consumeVerifyRateLimit(params: {
  scopeType: "user" | "ip";
  scopeKey: string;
  capacity: number;
  refillPerSec: number;
  nowMs: bigint;
  cost?: number;
}): Promise<{ allowed: boolean; retryAfterMs: number }> {
  const pool = getPool();
  const client = await pool.connect();
  const cost = params.cost ?? 1;
  try {
    await client.query("BEGIN");
    const row = await client.query<{ tokens: number; updated_at: string | number }>(
      "SELECT tokens, updated_at FROM verify_rate_limits WHERE scope_type = $1 AND scope_key = $2 FOR UPDATE",
      [params.scopeType, params.scopeKey]
    );
    let tokens = params.capacity;
    let updatedAt = params.nowMs;
    const first = row.rows[0];
    if (first) {
      const prevTokens = Number(first.tokens);
      const prevUpdatedAt = BigInt(first.updated_at);
      const elapsedMs = Number(params.nowMs - prevUpdatedAt);
      const refill = Math.max(0, elapsedMs) * (params.refillPerSec / 1000);
      tokens = Math.min(params.capacity, prevTokens + refill);
      updatedAt = params.nowMs;
    }
    const allowed = tokens >= cost;
    const nextTokens = allowed ? tokens - cost : tokens;
    await client.query(
      "INSERT INTO verify_rate_limits(scope_type, scope_key, tokens, updated_at) VALUES($1, $2, $3, $4) ON CONFLICT(scope_type, scope_key) DO UPDATE SET tokens = EXCLUDED.tokens, updated_at = EXCLUDED.updated_at",
      [params.scopeType, params.scopeKey, nextTokens, updatedAt.toString()]
    );
    await client.query("COMMIT");
    if (allowed) {
      return { allowed: true, retryAfterMs: 0 };
    }
    const deficit = cost - tokens;
    const retryAfterSec = Math.ceil(deficit / params.refillPerSec);
    return { allowed: false, retryAfterMs: retryAfterSec * 1000 };
  } catch (err) {
    await client.query("ROLLBACK");
    throw err;
  } finally {
    client.release();
  }
}

export async function addVerifyMetricsSample(params: {
  sampledAtMs: bigint;
  windowMs: bigint;
  retentionCutoffMs: bigint;
}): Promise<void> {
  const pool = getPool();
  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    const windowStart = params.sampledAtMs - params.windowMs;
    const queueRow = await client.query<{ n: string | number }>(
      "SELECT COUNT(*)::bigint AS n FROM verify_requests WHERE status IN ('queued', 'running')"
    );
    const statsRow = await client.query<{
      success_count: string | number;
      failed_count: string | number;
      avg_duration_ms: string | number | null;
      p50_duration_ms: string | number | null;
      p95_duration_ms: string | number | null;
    }>(
      "SELECT COUNT(*) FILTER (WHERE status = 'succeeded')::bigint AS success_count, COUNT(*) FILTER (WHERE status = 'failed')::bigint AS failed_count, AVG(CASE WHEN started_at IS NOT NULL AND finished_at IS NOT NULL THEN finished_at - started_at END)::bigint AS avg_duration_ms, PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY CASE WHEN started_at IS NOT NULL AND finished_at IS NOT NULL THEN finished_at - started_at END)::bigint AS p50_duration_ms, PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY CASE WHEN started_at IS NOT NULL AND finished_at IS NOT NULL THEN finished_at - started_at END)::bigint AS p95_duration_ms FROM verify_requests WHERE finished_at IS NOT NULL AND finished_at > $1 AND finished_at <= $2",
      [windowStart.toString(), params.sampledAtMs.toString()]
    );
    const failRows = await client.query<{ error_code: string | null; count: string | number }>(
      "SELECT error_code, COUNT(*)::bigint AS count FROM verify_requests WHERE status = 'failed' AND finished_at IS NOT NULL AND finished_at > $1 AND finished_at <= $2 GROUP BY error_code",
      [windowStart.toString(), params.sampledAtMs.toString()]
    );
    const failByCode: Record<string, string> = {};
    for (const row of failRows.rows) {
      const key = row.error_code ?? "unknown";
      failByCode[key] = String(row.count);
    }
    const queueDepth = BigInt(queueRow.rows[0]?.n ?? 0);
    const stats = statsRow.rows[0];
    await client.query(
      "INSERT INTO verify_metrics_samples(sampled_at_ms, queue_depth, success_count, failed_count, avg_duration_ms, p50_duration_ms, p95_duration_ms, fail_by_code_json) VALUES($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT(sampled_at_ms) DO UPDATE SET queue_depth = EXCLUDED.queue_depth, success_count = EXCLUDED.success_count, failed_count = EXCLUDED.failed_count, avg_duration_ms = EXCLUDED.avg_duration_ms, p50_duration_ms = EXCLUDED.p50_duration_ms, p95_duration_ms = EXCLUDED.p95_duration_ms, fail_by_code_json = EXCLUDED.fail_by_code_json",
      [
        params.sampledAtMs.toString(),
        queueDepth.toString(),
        String(stats?.success_count ?? "0"),
        String(stats?.failed_count ?? "0"),
        stats?.avg_duration_ms === null || stats?.avg_duration_ms === undefined ? null : String(stats.avg_duration_ms),
        stats?.p50_duration_ms === null || stats?.p50_duration_ms === undefined ? null : String(stats.p50_duration_ms),
        stats?.p95_duration_ms === null || stats?.p95_duration_ms === undefined ? null : String(stats.p95_duration_ms),
        JSON.stringify(failByCode),
      ]
    );
    await client.query("DELETE FROM verify_metrics_samples WHERE sampled_at_ms < $1", [
      params.retentionCutoffMs.toString(),
    ]);
    await client.query("COMMIT");
  } catch (err) {
    await client.query("ROLLBACK");
    throw err;
  } finally {
    client.release();
  }
}

export async function getVerifyMetricsSamplesSince(sinceMs: bigint): Promise<VerifyMetricsSample[]> {
  const pool = getPool();
  const out = await pool.query<{
    sampled_at_ms: string | number;
    queue_depth: string | number;
    success_count: string | number;
    failed_count: string | number;
    avg_duration_ms: string | number | null;
    p50_duration_ms: string | number | null;
    p95_duration_ms: string | number | null;
    fail_by_code_json: string;
  }>(
    "SELECT sampled_at_ms, queue_depth, success_count, failed_count, avg_duration_ms, p50_duration_ms, p95_duration_ms, fail_by_code_json FROM verify_metrics_samples WHERE sampled_at_ms >= $1 ORDER BY sampled_at_ms DESC",
    [sinceMs.toString()]
  );
  return out.rows.map((row) => ({
    sampledAtMs: BigInt(row.sampled_at_ms),
    queueDepth: BigInt(row.queue_depth),
    successCount: BigInt(row.success_count),
    failedCount: BigInt(row.failed_count),
    avgDurationMs: row.avg_duration_ms === null ? null : BigInt(row.avg_duration_ms),
    p50DurationMs: row.p50_duration_ms === null ? null : BigInt(row.p50_duration_ms),
    p95DurationMs: row.p95_duration_ms === null ? null : BigInt(row.p95_duration_ms),
    failByCodeJson: row.fail_by_code_json,
  }));
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

function isPgUniqueViolation(err: unknown): boolean {
  if (typeof err !== "object" || err === null || Array.isArray(err)) {
    return false;
  }
  return (err as { code?: string }).code === "23505";
}

function mapVerifyRequestRow(row: {
  id: string;
  contract_address: string;
  chain_id: number;
  submitted_by: string;
  status: VerifyStatus;
  input_hash: string;
  payload_compressed: Buffer;
  error_code: string | null;
  error_message: string | null;
  started_at: string | number | null;
  finished_at: string | number | null;
  attempts: number;
  created_at: string | number;
  updated_at: string | number;
  verified_contract_id: string | null;
}): VerifyRequest {
  return {
    id: row.id,
    contractAddress: row.contract_address,
    chainId: row.chain_id,
    submittedBy: row.submitted_by,
    status: row.status,
    inputHash: row.input_hash,
    payloadCompressed: new Uint8Array(row.payload_compressed),
    errorCode: row.error_code,
    errorMessage: row.error_message,
    startedAt: row.started_at === null ? null : BigInt(row.started_at),
    finishedAt: row.finished_at === null ? null : BigInt(row.finished_at),
    attempts: row.attempts,
    createdAt: BigInt(row.created_at),
    updatedAt: BigInt(row.updated_at),
    verifiedContractId: row.verified_contract_id,
  };
}
