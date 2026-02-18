// どこで: Explorerデータ取得層 / 何を: PostgresとRPCの問い合わせをユースケース単位で束ねる / なぜ: page側の分岐ロジックを最小化するため

import { Principal } from "@dfinity/principal";
import { loadConfig } from "./config";
import { extractErc20TransfersFromReceipt, type Erc20TransferView } from "./erc20";
import { getTokenMeta } from "./token_meta";
import {
  getBlockDetails,
  getLatestBlocks,
  getLatestBlocksPage,
  getLatestTxs,
  getLatestTxsPage,
  getMaxBlockNumber,
  getMetaSnapshot,
  getOpsMetricsSamplesSince,
  getOverviewStats,
  getRecentOpsMetricsSamples,
  getTokenTransfersByAddress,
  getTx,
  getTxsByAddress,
  getTxsByCallerPrincipal,
  type BlockSummary,
  type OverviewStats,
  type TxSummary,
} from "./db";
import {
  ADDRESS_HISTORY_LIMIT,
  buildAddressCursor,
  buildTokenTransferCursor,
  mapAddressTokenTransfers,
  mapAddressHistory,
  parseTokenTransferCursor,
  parseAddressCursor,
  type AddressView,
} from "./data_address";
import {
  buildPruneHistory,
  buildOpsSeries,
  detectPendingStall,
  pruneStatusFromLive,
  type OpsSeriesPoint,
  type PruneHistoryPoint,
  type StoredPruneStatus,
} from "./data_ops";
import { bytesToBigInt, isAddressHex, normalizeHex, parseAddressHex, parseHex, toHexLower } from "./hex";
import { deriveEvmAddressFromPrincipal } from "./principal";
import {
  getReceiptByTxId,
  getRpcBalance,
  getRpcBlock,
  getRpcCode,
  getRpcExpectedNonce,
  getRpcHeadNumber,
  getRpcPruneStatus,
  getRpcTxByTxId,
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
  blockLimit: number;
};

export type LatestTxsPageView = {
  txs: TxSummaryWithPrincipal[];
  page: number;
  limit: number;
  hasPrev: boolean;
  hasNext: boolean;
};

export type LatestBlocksPageView = {
  blocks: BlockSummary[];
  page: number;
  limit: number;
  hasPrev: boolean;
  hasNext: boolean;
};

export type { AddressView, OpsSeriesPoint, StoredPruneStatus };

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
  pruneHistory: PruneHistoryPoint[];
  capacityTrendSeries: Array<{
    sampledAtMs: bigint;
    estimatedKeptBytes: bigint;
    highWaterBytes: bigint;
    hardEmergencyBytes: bigint;
  }>;
  capacity: {
    estimatedKeptBytes: bigint | null;
    lowWaterBytes: bigint | null;
    highWaterBytes: bigint | null;
    hardEmergencyBytes: bigint | null;
    highWaterRatio: number | null;
    hardEmergencyRatio: number | null;
    forecast24h: CapacityForecast;
    forecast7d: CapacityForecast;
  };
  cyclesTrendSeries: OpsSeriesPoint[];
  series: OpsSeriesPoint[];
  pendingStall: boolean;
  warnings: string[];
  memoryBreakdown: StoredMemoryBreakdown | null;
};

type CapacityForecast = {
  growthBytesPerDay: number | null;
  daysToHighWater: number | null;
  daysToHardEmergency: number | null;
};

type StoredMemoryRegion = {
  id: number;
  name: string;
  pages: bigint;
  bytes: bigint;
};

type StoredMemoryBreakdown = {
  fetchedAtMs: bigint | null;
  stablePagesTotal: bigint;
  stableBytesTotal: bigint;
  regionsPagesTotal: bigint;
  regionsBytesTotal: bigint;
  unattributedStablePages: bigint;
  unattributedStableBytes: bigint;
  heapPages: bigint;
  heapBytes: bigint;
  regions: StoredMemoryRegion[];
};

export type CyclesTrendWindow = "24h" | "7d";

export type PrincipalView = {
  principalText: string;
  derivedAddressHex: string;
  address: AddressView;
  txs: TxSummaryWithPrincipal[];
};

export type TxSummaryWithPrincipal = TxSummary & {
  callerPrincipalText: string | null;
};

export type TxDetailView = {
  tx: TxSummaryWithPrincipal;
  statusLabel: string;
  valueWei: bigint | null;
  effectiveGasPriceWei: bigint | null;
  transactionFeeWei: bigint | null;
  receipt: ReceiptView | null;
  receiptLookupError: LookupError | null;
  erc20Transfers: Array<Erc20TransferView & { tokenSymbol: string | null; tokenDecimals: number | null }>;
};

type BlockDetailsWithPrincipal = {
  block: BlockSummary;
  txs: TxSummaryWithPrincipal[];
};

export type BlockGasView = {
  gasLimit: bigint | null;
  gasUsed: bigint | null;
  baseFeePerGasWei: bigint | null;
  burntFeesWei: bigint | null;
};

const HOME_BLOCKS_LIMIT_MAX = 500;
const TXS_PAGE_LIMIT_DEFAULT = 50;
const TXS_PAGE_LIMIT_MAX = 100;
const BLOCKS_PAGE_LIMIT_MAX = 100;
const OPS_TIMESERIES_TABLE_LIMIT = 10;
const OPS_PRUNE_HISTORY_LIMIT = 10;
const CYCLES_TREND_WINDOW_MS_24H = 24 * 60 * 60 * 1000;
const CYCLES_TREND_WINDOW_MS_7D = 7 * 24 * 60 * 60 * 1000;
const DAY_MS = 24 * 60 * 60 * 1000;

function parsePositiveInt(rawValue: string | string[] | undefined, fallback: number): number {
  const raw = Array.isArray(rawValue) ? rawValue[0] : rawValue;
  if (!raw || !/^\d+$/.test(raw)) {
    return fallback;
  }
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed < 1) {
    return fallback;
  }
  return parsed;
}

export function resolveHomeBlocksLimit(rawValue: string | string[] | undefined, fallback: number): number {
  const raw = Array.isArray(rawValue) ? rawValue[0] : rawValue;
  if (!raw) {
    return fallback;
  }
  if (!/^\d+$/.test(raw)) {
    return fallback;
  }
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed < 1 || parsed > HOME_BLOCKS_LIMIT_MAX) {
    return fallback;
  }
  return parsed;
}

export async function getHomeView(blocksLimitRaw?: string | string[]): Promise<HomeView> {
  const cfg = loadConfig(process.env);
  const blockLimit = resolveHomeBlocksLimit(blocksLimitRaw, cfg.latestBlocksLimit);
  const [rpcHead, dbHead, stats, blocks, txs] = await Promise.all([
    getRpcHeadNumber(),
    getMaxBlockNumber(),
    getOverviewStats(),
    getLatestBlocks(blockLimit),
    getLatestTxs(cfg.latestTxsLimit),
  ]);
  return { rpcHead, dbHead, stats, blocks, txs: withCallerPrincipalText(txs), blockLimit };
}

export async function getLatestTxsPageView(
  pageRaw?: string | string[],
  limitRaw?: string | string[]
): Promise<LatestTxsPageView> {
  const page = parsePositiveInt(pageRaw, 1);
  const limitParsed = parsePositiveInt(limitRaw, TXS_PAGE_LIMIT_DEFAULT);
  const limit = Math.min(limitParsed, TXS_PAGE_LIMIT_MAX);
  const offset = (page - 1) * limit;
  const [txsRaw, stats] = await Promise.all([getLatestTxsPage(limit, offset), getOverviewStats()]);
  const total = Number(stats.totalTxs);
  const nextOffset = page * limit;
  return {
    txs: withCallerPrincipalText(txsRaw),
    page,
    limit,
    hasPrev: page > 1,
    hasNext: Number.isFinite(total) ? nextOffset < total : txsRaw.length === limit,
  };
}

export async function getLatestBlocksPageView(
  pageRaw?: string | string[],
  limitRaw?: string | string[]
): Promise<LatestBlocksPageView> {
  const page = parsePositiveInt(pageRaw, 1);
  const limitParsed = parsePositiveInt(limitRaw, 50);
  const limit = Math.min(limitParsed, BLOCKS_PAGE_LIMIT_MAX);
  const offset = (page - 1) * limit;
  const [blocks, stats] = await Promise.all([getLatestBlocksPage(limit, offset), getOverviewStats()]);
  const total = Number(stats.totalBlocks);
  const nextOffset = page * limit;
  return {
    blocks,
    page,
    limit,
    hasPrev: page > 1,
    hasNext: Number.isFinite(total) ? nextOffset < total : blocks.length === limit,
  };
}

export async function getBlockView(
  blockNumber: bigint
): Promise<{ db: BlockDetailsWithPrincipal | null; rpcExists: boolean; rpcGas: BlockGasView | null }> {
  const [db, rpcBlock] = await Promise.all([getBlockDetails(blockNumber), getRpcBlock(blockNumber)]);
  const rpcGas = rpcBlock
    ? {
        gasLimit: rpcBlock.gas_limit[0] ?? null,
        gasUsed: rpcBlock.gas_used[0] ?? null,
        baseFeePerGasWei: rpcBlock.base_fee_per_gas[0] ?? null,
        burntFeesWei:
          rpcBlock.gas_used[0] !== undefined && rpcBlock.base_fee_per_gas[0] !== undefined
            ? rpcBlock.gas_used[0] * rpcBlock.base_fee_per_gas[0]
            : null,
      }
    : null;
  if (!db) {
    return { db: null, rpcExists: rpcBlock !== null, rpcGas };
  }
  return {
    db: {
      block: db.block,
      txs: withCallerPrincipalText(db.txs),
    },
    rpcExists: rpcBlock !== null,
    rpcGas,
  };
}

export async function getTxView(txHashHex: string): Promise<TxSummary | null> {
  return getTx(parseHex(txHashHex));
}

export async function getTxDetailView(txHashHex: string): Promise<TxDetailView | null> {
  const txId = parseHex(txHashHex);
  const [txRaw, receiptOut, rpcTx] = await Promise.all([
    getTx(txId),
    getReceiptByTxId(txId),
    getRpcTxByTxId(txId),
  ]);
  if (!txRaw) {
    return null;
  }
  const tx = withCallerPrincipalText([txRaw])[0];
  if (!tx) {
    return null;
  }
  const valueWei = rpcTx?.decoded[0] ? bytesToBigInt(rpcTx.decoded[0].value) : null;
  const effectiveGasPriceWei = "Ok" in receiptOut ? receiptOut.Ok.effective_gas_price : null;
  const transactionFeeWei = "Ok" in receiptOut ? receiptOut.Ok.total_fee : null;
  const erc20TransfersRaw = "Ok" in receiptOut ? extractErc20TransfersFromReceipt(receiptOut.Ok) : [];
  const erc20Transfers = await withTokenMeta(erc20TransfersRaw);
  return {
    tx,
    statusLabel: receiptStatusLabel(tx.receiptStatus),
    valueWei,
    effectiveGasPriceWei,
    transactionFeeWei,
    receipt: "Ok" in receiptOut ? receiptOut.Ok : null,
    receiptLookupError: "Ok" in receiptOut ? null : receiptOut.Err,
    erc20Transfers,
  };
}

export async function getAddressView(
  addressHex: string,
  cursorToken?: string,
  tokenCursorToken?: string,
  providedPrincipal?: string | null
): Promise<AddressView> {
  if (!isAddressHex(addressHex)) {
    throw new Error("address must be a 20-byte hex string");
  }
  const normalized = normalizeHex(addressHex);
  const bytes = parseAddressHex(normalized);
  const cursor = parseAddressCursor(cursorToken);
  const tokenCursor = parseTokenTransferCursor(tokenCursorToken);
  const warnings: string[] = [];
  const [balance, nonce, code, historyRows, tokenTransferRows] = await Promise.all([
    tryRpc(() => getRpcBalance(bytes), "balance RPC is unavailable", warnings),
    tryRpc(() => getRpcExpectedNonce(bytes), "nonce RPC is unavailable", warnings),
    tryRpc(() => getRpcCode(bytes), "code RPC is unavailable", warnings),
    getTxsByAddress(bytes, ADDRESS_HISTORY_LIMIT, cursor),
    getTokenTransfersByAddress(bytes, ADDRESS_HISTORY_LIMIT, tokenCursor),
  ]);

  const codeBytes = code ? code.length : null;
  const hasMore = historyRows.length > ADDRESS_HISTORY_LIMIT;
  const pageRows = hasMore ? historyRows.slice(0, ADDRESS_HISTORY_LIMIT) : historyRows;
  const nextCursorRow = hasMore ? pageRows[pageRows.length - 1] : undefined;
  const nextCursor = nextCursorRow ? buildAddressCursor(nextCursorRow) : null;
  const history = mapAddressHistory(pageRows, toHexLower(bytes));
  const tokenHasMore = tokenTransferRows.length > ADDRESS_HISTORY_LIMIT;
  const tokenPageRows = tokenHasMore ? tokenTransferRows.slice(0, ADDRESS_HISTORY_LIMIT) : tokenTransferRows;
  const tokenNextCursorRow = tokenHasMore ? tokenPageRows[tokenPageRows.length - 1] : undefined;
  const tokenNextCursor = tokenNextCursorRow ? buildTokenTransferCursor(tokenNextCursorRow) : null;
  const tokenTransfers = mapAddressTokenTransfers(tokenPageRows, toHexLower(bytes));
  const observedPrincipals = collectObservedPrincipals(pageRows);

  return {
    addressHex: toHexLower(bytes),
    providedPrincipal: providedPrincipal ?? null,
    observedPrincipals,
    balance,
    nonce,
    codeBytes,
    isContract: codeBytes === null ? null : codeBytes > 0,
    history,
    failedHistory: history.filter((row) => row.receiptStatus === 0),
    nextCursor,
    tokenTransfers,
    tokenNextCursor,
    warnings,
  };
}

export function parseCyclesTrendWindow(raw: string | undefined): CyclesTrendWindow {
  return raw === "7d" ? "7d" : "24h";
}

export async function getOpsView(cyclesTrendWindow: CyclesTrendWindow = "24h"): Promise<OpsView> {
  const warnings: string[] = [];
  const nowMs = Date.now();
  const windowMs = cyclesTrendWindow === "7d" ? CYCLES_TREND_WINDOW_MS_7D : CYCLES_TREND_WINDOW_MS_24H;
  const cyclesTrendSinceMs = BigInt(nowMs - windowMs);
  const [rpcHead, dbHead, stats, meta, pruneStatusLive, samples, cyclesTrendSamples, capacityForecastSamples] = await Promise.all([
    tryRpc(() => getRpcHeadNumber(), "rpc head is unavailable", warnings),
    getMaxBlockNumber(),
    getOverviewStats(),
    getMetaSnapshot(),
    tryRpc(() => getRpcPruneStatus(), "live prune status is unavailable", warnings),
    getRecentOpsMetricsSamples(OPS_TIMESERIES_TABLE_LIMIT),
    getOpsMetricsSamplesSince(cyclesTrendSinceMs),
    getOpsMetricsSamplesSince(BigInt(nowMs - CYCLES_TREND_WINDOW_MS_7D)),
  ]);

  const effectiveNeedPrune = pruneStatusLive ? pruneStatusLive.need_prune : null;
  const effectiveStoredPruneStatus = pruneStatusFromLive(pruneStatusLive);
  const memoryBreakdown = parseStoredMemoryBreakdown(meta.memoryBreakdownRaw);
  const capacity = buildCapacityView(effectiveStoredPruneStatus, capacityForecastSamples);
  const cyclesTrendSeries = buildOpsSeries(cyclesTrendSamples);
  const series = buildOpsSeries(samples);
  const capacityTrendSeries = buildCapacityTrendSeries(cyclesTrendSamples);
  const pruneHistory = buildPruneHistory(samples, OPS_PRUNE_HISTORY_LIMIT);

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
    pruneHistory,
    capacityTrendSeries,
    capacity,
    cyclesTrendSeries,
    series,
    pendingStall: detectPendingStall(series, 15 * 60 * 1000),
    warnings,
    memoryBreakdown,
  };
}

function buildCapacityTrendSeries(
  samples: Array<{
    sampledAtMs: bigint;
    estimatedKeptBytes: bigint | null;
    highWaterBytes: bigint | null;
    hardEmergencyBytes: bigint | null;
  }>
): OpsView["capacityTrendSeries"] {
  const out: OpsView["capacityTrendSeries"] = [];
  for (const sample of [...samples].reverse()) {
    if (
      sample.estimatedKeptBytes === null ||
      sample.highWaterBytes === null ||
      sample.hardEmergencyBytes === null
    ) {
      continue;
    }
    out.push({
      sampledAtMs: sample.sampledAtMs,
      estimatedKeptBytes: sample.estimatedKeptBytes,
      highWaterBytes: sample.highWaterBytes,
      hardEmergencyBytes: sample.hardEmergencyBytes,
    });
  }
  return out;
}

function buildCapacityView(
  pruneStatus: StoredPruneStatus | null,
  samples: Array<{ sampledAtMs: bigint; estimatedKeptBytes: bigint | null }>
): OpsView["capacity"] {
  const status = pruneStatus?.status ?? null;
  const forecast24h = buildCapacityForecast(samples, CYCLES_TREND_WINDOW_MS_24H, status?.highWaterBytes ?? null, status?.hardEmergencyBytes ?? null);
  const forecast7d = buildCapacityForecast(samples, CYCLES_TREND_WINDOW_MS_7D, status?.highWaterBytes ?? null, status?.hardEmergencyBytes ?? null);
  if (!status) {
    return {
      estimatedKeptBytes: null,
      lowWaterBytes: null,
      highWaterBytes: null,
      hardEmergencyBytes: null,
      highWaterRatio: null,
      hardEmergencyRatio: null,
      forecast24h,
      forecast7d,
    };
  }
  return {
    estimatedKeptBytes: status.estimatedKeptBytes,
    lowWaterBytes: status.lowWaterBytes,
    highWaterBytes: status.highWaterBytes,
    hardEmergencyBytes: status.hardEmergencyBytes,
    highWaterRatio: ratioOrNull(status.estimatedKeptBytes, status.highWaterBytes),
    hardEmergencyRatio: ratioOrNull(status.estimatedKeptBytes, status.hardEmergencyBytes),
    forecast24h,
    forecast7d,
  };
}

function buildCapacityForecast(
  samples: Array<{ sampledAtMs: bigint; estimatedKeptBytes: bigint | null }>,
  windowMs: number,
  highWaterBytes: bigint | null,
  hardEmergencyBytes: bigint | null
): CapacityForecast {
  const estimatedSeries = [...samples]
    .filter((sample) => sample.estimatedKeptBytes !== null)
    .map((sample) => ({
      sampledAtMs: sample.sampledAtMs,
      estimatedKeptBytes: sample.estimatedKeptBytes ?? 0n,
    }))
    .sort((a, b) => Number(a.sampledAtMs - b.sampledAtMs));
  if (estimatedSeries.length < 2) {
    return { growthBytesPerDay: null, daysToHighWater: null, daysToHardEmergency: null };
  }
  const newest = estimatedSeries[estimatedSeries.length - 1];
  if (!newest) {
    return { growthBytesPerDay: null, daysToHighWater: null, daysToHardEmergency: null };
  }
  const windowStart = newest.sampledAtMs - BigInt(windowMs);
  const inWindow = estimatedSeries.filter((point) => point.sampledAtMs >= windowStart);
  if (inWindow.length < 2) {
    return { growthBytesPerDay: null, daysToHighWater: null, daysToHardEmergency: null };
  }
  const first = inWindow[0];
  const last = inWindow[inWindow.length - 1];
  if (!first || !last) {
    return { growthBytesPerDay: null, daysToHighWater: null, daysToHardEmergency: null };
  }
  const deltaMs = Number(last.sampledAtMs - first.sampledAtMs);
  if (!Number.isFinite(deltaMs) || deltaMs <= 0) {
    return { growthBytesPerDay: null, daysToHighWater: null, daysToHardEmergency: null };
  }
  const deltaBytes = Number(last.estimatedKeptBytes - first.estimatedKeptBytes);
  if (!Number.isFinite(deltaBytes)) {
    return { growthBytesPerDay: null, daysToHighWater: null, daysToHardEmergency: null };
  }
  const growthBytesPerDay = (deltaBytes * DAY_MS) / deltaMs;
  return {
    growthBytesPerDay,
    daysToHighWater: daysToThreshold(last.estimatedKeptBytes, highWaterBytes, growthBytesPerDay),
    daysToHardEmergency: daysToThreshold(last.estimatedKeptBytes, hardEmergencyBytes, growthBytesPerDay),
  };
}

function daysToThreshold(current: bigint, threshold: bigint | null, growthBytesPerDay: number): number | null {
  if (threshold === null) {
    return null;
  }
  if (current >= threshold) {
    return 0;
  }
  if (!Number.isFinite(growthBytesPerDay) || growthBytesPerDay <= 0) {
    return null;
  }
  const remainingBytes = Number(threshold - current);
  if (!Number.isFinite(remainingBytes) || remainingBytes <= 0) {
    return 0;
  }
  return remainingBytes / growthBytesPerDay;
}

function ratioOrNull(numerator: bigint, denominator: bigint): number | null {
  if (denominator <= 0n) {
    return null;
  }
  const ratioBps = (numerator * 10_000n) / denominator;
  return Number(ratioBps) / 10_000;
}

export const opsDataTestHooks = {
  buildCapacityForecast,
};

export async function getPrincipalView(principalText: string): Promise<PrincipalView> {
  const principalBytes = Principal.fromText(principalText).toUint8Array();
  const derivedAddressHex = deriveEvmAddressFromPrincipal(principalText);
  const cfg = loadConfig(process.env);
  const [address, txs] = await Promise.all([
    getAddressView(derivedAddressHex),
    getTxsByCallerPrincipal(principalBytes, cfg.principalTxsLimit),
  ]);
  return {
    principalText,
    derivedAddressHex,
    address,
    txs: withCallerPrincipalText(txs),
  };
}

function withCallerPrincipalText(txs: TxSummary[]): TxSummaryWithPrincipal[] {
  return txs.map((tx) => {
    if (!tx.callerPrincipal) {
      return { ...tx, callerPrincipalText: null };
    }
    try {
      return { ...tx, callerPrincipalText: Principal.fromUint8Array(tx.callerPrincipal).toText() };
    } catch {
      return { ...tx, callerPrincipalText: null };
    }
  });
}

function parseStoredMemoryBreakdown(raw: string | null): StoredMemoryBreakdown | null {
  if (!raw) {
    return null;
  }
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!isRecord(parsed)) {
      return null;
    }
    const breakdownRaw = parsed.breakdown;
    if (!isRecord(breakdownRaw)) {
      return null;
    }
    const fetchedAtMs = toBigIntOrNull(parsed.fetched_at_ms);
    const stablePagesTotal = toBigIntOrNull(breakdownRaw.stable_pages_total);
    const stableBytesTotal = toBigIntOrNull(breakdownRaw.stable_bytes_total);
    const regionsPagesTotal = toBigIntOrNull(breakdownRaw.regions_pages_total);
    const regionsBytesTotal = toBigIntOrNull(breakdownRaw.regions_bytes_total);
    const unattributedStablePages = toBigIntOrNull(breakdownRaw.unattributed_stable_pages);
    const unattributedStableBytes = toBigIntOrNull(breakdownRaw.unattributed_stable_bytes);
    const heapPages = toBigIntOrNull(breakdownRaw.heap_pages);
    const heapBytes = toBigIntOrNull(breakdownRaw.heap_bytes);
    const regionsRaw = breakdownRaw.regions;
    if (
      stablePagesTotal === null ||
      stableBytesTotal === null ||
      regionsPagesTotal === null ||
      regionsBytesTotal === null ||
      unattributedStablePages === null ||
      unattributedStableBytes === null ||
      heapPages === null ||
      heapBytes === null ||
      !Array.isArray(regionsRaw)
    ) {
      return null;
    }
    const regions: StoredMemoryRegion[] = [];
    for (const item of regionsRaw) {
      if (!isRecord(item)) {
        continue;
      }
      const id = toNumberOrNull(item.id);
      const name = typeof item.name === "string" ? item.name : null;
      const pages = toBigIntOrNull(item.pages);
      const bytes = toBigIntOrNull(item.bytes);
      if (id === null || name === null || pages === null || bytes === null) {
        continue;
      }
      regions.push({ id, name, pages, bytes });
    }
    return {
      fetchedAtMs,
      stablePagesTotal,
      stableBytesTotal,
      regionsPagesTotal,
      regionsBytesTotal,
      unattributedStablePages,
      unattributedStableBytes,
      heapPages,
      heapBytes,
      regions,
    };
  } catch {
    return null;
  }
}

async function tryRpc<T>(call: () => Promise<T>, warningMessage: string, warnings: string[]): Promise<T | null> {
  try {
    return await call();
  } catch {
    warnings.push(warningMessage);
    return null;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
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

function toNumberOrNull(value: unknown): number | null {
  if (typeof value === "number" && Number.isInteger(value)) {
    return value;
  }
  if (typeof value === "bigint") {
    const asNumber = Number(value);
    if (Number.isInteger(asNumber)) {
      return asNumber;
    }
    return null;
  }
  if (typeof value === "string" && /^(0|[1-9][0-9]*)$/.test(value)) {
    const parsed = Number(value);
    if (Number.isInteger(parsed)) {
      return parsed;
    }
  }
  return null;
}

async function withTokenMeta(
  transfers: Erc20TransferView[]
): Promise<Array<Erc20TransferView & { tokenSymbol: string | null; tokenDecimals: number | null }>> {
  const unique = new Set<string>();
  for (const item of transfers) {
    unique.add(item.tokenAddressHex);
  }
  const entries = await Promise.all(
    Array.from(unique).map(async (tokenAddressHex) => ({ tokenAddressHex, meta: await getTokenMeta(tokenAddressHex) }))
  );
  const metaByToken = new Map<string, { symbol: string | null; decimals: number | null }>();
  for (const entry of entries) {
    metaByToken.set(entry.tokenAddressHex, entry.meta);
  }
  return transfers.map((item) => {
    const meta = metaByToken.get(item.tokenAddressHex) ?? { symbol: null, decimals: null };
    return {
      ...item,
      tokenSymbol: meta.symbol,
      tokenDecimals: meta.decimals,
    };
  });
}

function receiptStatusLabel(status: number | null): string {
  if (status === null) {
    return "unknown";
  }
  return status === 1 ? "success" : "failed";
}

function collectObservedPrincipals(rows: TxSummary[]): string[] {
  const unique = new Set<string>();
  for (const row of rows) {
    if (!row.callerPrincipal) {
      continue;
    }
    try {
      unique.add(Principal.fromUint8Array(row.callerPrincipal).toText());
    } catch {
      // 破損データは表示候補から除外して探索継続する。
    }
    if (unique.size >= 8) {
      break;
    }
  }
  return Array.from(unique);
}
