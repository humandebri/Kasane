// どこで: Explorerデータ取得層 / 何を: PostgresとRPCの問い合わせをユースケース単位で束ねる / なぜ: page側の分岐ロジックを最小化するため

import { loadConfig } from "./config";
import {
  getMetaSnapshot,
  getBlockDetails,
  getLatestBlocks,
  getLatestTxs,
  getMaxBlockNumber,
  getOverviewStats,
  getTx,
  getTxsByCallerPrincipal,
  type BlockSummary,
  type OverviewStats,
  type TxSummary,
} from "./db";
import { Principal } from "@dfinity/principal";
import {
  isAddressHex,
  normalizeHex,
  parseAddressHex,
  parseHex,
  toHexLower,
} from "./hex";
import {
  getRpcBalance,
  getReceiptByTxId,
  getRpcBlock,
  getRpcCode,
  getRpcExpectedNonce,
  getRpcHeadNumber,
  getRpcPruneStatus,
  type LookupError,
  type PruneStatusView,
  type ReceiptView,
} from "./rpc";

type HomeView = {
  dbHead: bigint | null;
  rpcHead: bigint;
  stats: OverviewStats;
  blocks: BlockSummary[];
  txs: TxSummaryWithPrincipal[];
};

export type AddressView = {
  addressHex: string;
  balance: bigint | null;
  nonce: bigint | null;
  codeBytes: number | null;
  isContract: boolean | null;
  warnings: string[];
};

export type StoredPruneStatus = {
  fetchedAtMs: bigint | null;
  status: {
    pruningEnabled: boolean;
    pruneRunning: boolean;
    needPrune: boolean;
    prunedBeforeBlock: bigint | null;
    oldestKeptBlock: bigint | null;
    oldestKeptTimestamp: bigint | null;
    estimatedKeptBytes: bigint;
    highWaterBytes: bigint;
    lowWaterBytes: bigint;
    hardEmergencyBytes: bigint;
    lastPruneAt: bigint;
  } | null;
};

export type OpsView = {
  rpcHead: bigint | null;
  dbHead: bigint | null;
  lag: bigint | null;
  stats: OverviewStats;
  needPrune: boolean | null;
  metaLastHead: bigint | null;
  lastIngestAtMs: bigint | null;
  pruneStatus: StoredPruneStatus | null;
  pruneStatusLive: PruneStatusView | null;
  warnings: string[];
};

export type PrincipalView = {
  principalText: string;
  txs: TxSummaryWithPrincipal[];
};

export type TxSummaryWithPrincipal = TxSummary & {
  callerPrincipalText: string | null;
};

type BlockDetailsWithPrincipal = {
  block: BlockSummary;
  txs: TxSummaryWithPrincipal[];
};

export async function getHomeView(): Promise<HomeView> {
  const cfg = loadConfig(process.env);
  const [rpcHead, dbHead, stats, blocks, txs] = await Promise.all([
    getRpcHeadNumber(),
    getMaxBlockNumber(),
    getOverviewStats(),
    getLatestBlocks(cfg.latestBlocksLimit),
    getLatestTxs(cfg.latestTxsLimit),
  ]);
  return { rpcHead, dbHead, stats, blocks, txs: withCallerPrincipalText(txs) };
}

export async function getBlockView(blockNumber: bigint): Promise<{ db: BlockDetailsWithPrincipal | null; rpcExists: boolean }> {
  const [db, rpcBlock] = await Promise.all([getBlockDetails(blockNumber), getRpcBlock(blockNumber)]);
  if (!db) {
    return { db: null, rpcExists: rpcBlock !== null };
  }
  return {
    db: {
      block: db.block,
      txs: withCallerPrincipalText(db.txs),
    },
    rpcExists: rpcBlock !== null,
  };
}

export async function getTxView(txHashHex: string): Promise<TxSummary | null> {
  const txHash = parseHex(txHashHex);
  return getTx(txHash);
}

export async function getReceiptView(
  txHashHex: string
): Promise<{ tx: TxSummary | null; receipt: ReceiptView | null; lookupError: LookupError | null }> {
  const txHash = parseHex(txHashHex);
  const [tx, out] = await Promise.all([getTx(txHash), getReceiptByTxId(txHash)]);
  if ("Ok" in out) {
    return { tx, receipt: out.Ok, lookupError: null };
  }
  return { tx, receipt: null, lookupError: out.Err };
}

export async function getAddressView(addressHex: string): Promise<AddressView> {
  if (!isAddressHex(addressHex)) {
    throw new Error("address must be a 20-byte hex string");
  }
  const normalized = normalizeHex(addressHex);
  const bytes = parseAddressHex(normalized);
  const warnings: string[] = [];
  const [balance, nonce, code] = await Promise.all([
    tryRpc(() => getRpcBalance(bytes), "balance RPC is unavailable", warnings),
    tryRpc(() => getRpcExpectedNonce(bytes), "nonce RPC is unavailable", warnings),
    tryRpc(() => getRpcCode(bytes), "code RPC is unavailable", warnings),
  ]);
  const codeBytes = code ? code.length : null;
  return {
    addressHex: toHexLower(bytes),
    balance,
    nonce,
    codeBytes,
    isContract: codeBytes === null ? null : codeBytes > 0,
    warnings,
  };
}

export async function getOpsView(): Promise<OpsView> {
  const warnings: string[] = [];
  const [rpcHead, dbHead, stats, meta, pruneStatusLive] = await Promise.all([
    tryRpc(() => getRpcHeadNumber(), "rpc head is unavailable", warnings),
    getMaxBlockNumber(),
    getOverviewStats(),
    getMetaSnapshot(),
    tryRpc(() => getRpcPruneStatus(), "live prune status is unavailable", warnings),
  ]);
  const pruneStatus = parseStoredPruneStatus(meta.pruneStatusRaw);
  const effectiveNeedPrune =
    meta.needPrune !== null ? meta.needPrune : pruneStatusLive ? pruneStatusLive.need_prune : null;
  const effectiveStoredPruneStatus = pruneStatus ?? pruneStatusFromLive(pruneStatusLive);
  return {
    rpcHead,
    dbHead,
    lag: rpcHead === null || dbHead === null ? null : rpcHead - dbHead,
    stats,
    needPrune: effectiveNeedPrune,
    metaLastHead: meta.lastHead,
    lastIngestAtMs: meta.lastIngestAtMs,
    pruneStatus: effectiveStoredPruneStatus,
    pruneStatusLive,
    warnings,
  };
}

export async function getPrincipalView(principalText: string): Promise<PrincipalView> {
  const principalBytes = Principal.fromText(principalText).toUint8Array();
  const cfg = loadConfig(process.env);
  const txs = await getTxsByCallerPrincipal(principalBytes, cfg.principalTxsLimit);
  return {
    principalText,
    txs: withCallerPrincipalText(txs),
  };
}

function withCallerPrincipalText(txs: TxSummary[]): TxSummaryWithPrincipal[] {
  return txs.map((tx) => {
    if (!tx.callerPrincipal) {
      return { ...tx, callerPrincipalText: null };
    }
    try {
      return {
        ...tx,
        callerPrincipalText: Principal.fromUint8Array(tx.callerPrincipal).toText(),
      };
    } catch {
      return { ...tx, callerPrincipalText: null };
    }
  });
}

async function tryRpc<T>(
  call: () => Promise<T>,
  warningMessage: string,
  warnings: string[]
): Promise<T | null> {
  try {
    return await call();
  } catch {
    warnings.push(warningMessage);
    return null;
  }
}

function parseStoredPruneStatus(raw: string | null): StoredPruneStatus | null {
  if (!raw) {
    return null;
  }
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!isRecord(parsed)) {
      return null;
    }
    const statusRaw = parsed.status;
    const fetchedAtMs = toBigIntOrNull(parsed.fetched_at_ms);
    if (!isRecord(statusRaw)) {
      return { fetchedAtMs, status: null };
    }
    const status = parseStoredPruneStatusRecord(statusRaw);
    return {
      fetchedAtMs,
      status,
    };
  } catch {
    return null;
  }
}

export function parseStoredPruneStatusForTest(raw: string | null): StoredPruneStatus | null {
  return parseStoredPruneStatus(raw);
}

function parseStoredPruneStatusRecord(
  statusRaw: Record<string, unknown>
): StoredPruneStatus["status"] {
  const pruningEnabled = toBoolOrNull(statusRaw.pruning_enabled);
  const pruneRunning = toBoolOrNull(statusRaw.prune_running);
  const needPrune = toBoolOrNull(statusRaw.need_prune);
  const estimatedKeptBytes = toBigIntOrNull(statusRaw.estimated_kept_bytes);
  const highWaterBytes = toBigIntOrNull(statusRaw.high_water_bytes);
  const lowWaterBytes = toBigIntOrNull(statusRaw.low_water_bytes);
  const hardEmergencyBytes = toBigIntOrNull(statusRaw.hard_emergency_bytes);
  const lastPruneAt = toBigIntOrNull(statusRaw.last_prune_at);
  if (
    pruningEnabled === null ||
    pruneRunning === null ||
    needPrune === null ||
    estimatedKeptBytes === null ||
    highWaterBytes === null ||
    lowWaterBytes === null ||
    hardEmergencyBytes === null ||
    lastPruneAt === null
  ) {
    return null;
  }
  return {
    pruningEnabled,
    pruneRunning,
    needPrune,
    prunedBeforeBlock: toBigIntOrNull(statusRaw.pruned_before_block),
    oldestKeptBlock: toBigIntOrNull(statusRaw.oldest_kept_block),
    oldestKeptTimestamp: toBigIntOrNull(statusRaw.oldest_kept_timestamp),
    estimatedKeptBytes,
    highWaterBytes,
    lowWaterBytes,
    hardEmergencyBytes,
    lastPruneAt,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function toBoolOrNull(value: unknown): boolean | null {
  if (value === true || value === "true" || value === "1") {
    return true;
  }
  if (value === false || value === "false" || value === "0") {
    return false;
  }
  return null;
}

function toBigIntOrNull(value: unknown): bigint | null {
  if (typeof value === "bigint") {
    return value;
  }
  if (typeof value === "number" && Number.isFinite(value) && Number.isInteger(value)) {
    return BigInt(value);
  }
  if (typeof value === "string" && value.trim() !== "") {
    try {
      return BigInt(value);
    } catch {
      return null;
    }
  }
  return null;
}

function pruneStatusFromLive(live: PruneStatusView | null): StoredPruneStatus | null {
  if (!live) {
    return null;
  }
  return {
    fetchedAtMs: null,
    status: {
      pruningEnabled: live.pruning_enabled,
      pruneRunning: live.prune_running,
      needPrune: live.need_prune,
      prunedBeforeBlock: live.pruned_before_block.length === 0 ? null : live.pruned_before_block[0],
      oldestKeptBlock: live.oldest_kept_block.length === 0 ? null : live.oldest_kept_block[0],
      oldestKeptTimestamp:
        live.oldest_kept_timestamp.length === 0 ? null : live.oldest_kept_timestamp[0],
      estimatedKeptBytes: live.estimated_kept_bytes,
      highWaterBytes: live.high_water_bytes,
      lowWaterBytes: live.low_water_bytes,
      hardEmergencyBytes: live.hard_emergency_bytes,
      lastPruneAt: live.last_prune_at,
    },
  };
}
