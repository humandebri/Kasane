// どこで: Explorerデータ取得層 / 何を: PostgresとRPCの問い合わせをユースケース単位で束ねる / なぜ: page側の分岐ロジックを最小化するため

import { Principal } from "@dfinity/principal";
import { loadConfig } from "./config";
import {
  getBlockDetails,
  getLatestBlocks,
  getLatestTxs,
  getMaxBlockNumber,
  getMetaSnapshot,
  getOverviewStats,
  getRecentOpsMetricsSamples,
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
  mapAddressHistory,
  parseAddressCursor,
  type AddressView,
} from "./data_address";
import {
  buildOpsSeries,
  detectPendingStall,
  parseStoredPruneStatus,
  pruneStatusFromLive,
  type OpsSeriesPoint,
  type StoredPruneStatus,
} from "./data_ops";
import { isAddressHex, normalizeHex, parseAddressHex, parseHex, toHexLower } from "./hex";
import { deriveEvmAddressFromPrincipal } from "./principal";
import {
  getReceiptByTxId,
  getRpcBalance,
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
  series: OpsSeriesPoint[];
  pendingStall: boolean;
  warnings: string[];
};

export type PrincipalView = {
  principalText: string;
  derivedAddressHex: string;
  address: AddressView;
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
  return getTx(parseHex(txHashHex));
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

export async function getAddressView(addressHex: string, cursorToken?: string): Promise<AddressView> {
  if (!isAddressHex(addressHex)) {
    throw new Error("address must be a 20-byte hex string");
  }
  const normalized = normalizeHex(addressHex);
  const bytes = parseAddressHex(normalized);
  const cursor = parseAddressCursor(cursorToken);
  const warnings: string[] = [];
  const [balance, nonce, code, historyRows] = await Promise.all([
    tryRpc(() => getRpcBalance(bytes), "balance RPC is unavailable", warnings),
    tryRpc(() => getRpcExpectedNonce(bytes), "nonce RPC is unavailable", warnings),
    tryRpc(() => getRpcCode(bytes), "code RPC is unavailable", warnings),
    getTxsByAddress(bytes, ADDRESS_HISTORY_LIMIT, cursor),
  ]);

  const codeBytes = code ? code.length : null;
  const hasMore = historyRows.length > ADDRESS_HISTORY_LIMIT;
  const pageRows = hasMore ? historyRows.slice(0, ADDRESS_HISTORY_LIMIT) : historyRows;
  const nextCursorRow = hasMore ? pageRows[pageRows.length - 1] : undefined;
  const nextCursor = nextCursorRow ? buildAddressCursor(nextCursorRow) : null;
  const history = mapAddressHistory(pageRows, toHexLower(bytes));

  return {
    addressHex: toHexLower(bytes),
    balance,
    nonce,
    codeBytes,
    isContract: codeBytes === null ? null : codeBytes > 0,
    history,
    failedHistory: history.filter((row) => row.receiptStatus === 0),
    nextCursor,
    warnings,
  };
}

export async function getOpsView(): Promise<OpsView> {
  const warnings: string[] = [];
  const [rpcHead, dbHead, stats, meta, pruneStatusLive, samples] = await Promise.all([
    tryRpc(() => getRpcHeadNumber(), "rpc head is unavailable", warnings),
    getMaxBlockNumber(),
    getOverviewStats(),
    getMetaSnapshot(),
    tryRpc(() => getRpcPruneStatus(), "live prune status is unavailable", warnings),
    getRecentOpsMetricsSamples(120),
  ]);

  const pruneStatus = parseStoredPruneStatus(meta.pruneStatusRaw);
  const effectiveNeedPrune =
    meta.needPrune !== null ? meta.needPrune : pruneStatusLive ? pruneStatusLive.need_prune : null;
  const effectiveStoredPruneStatus = pruneStatus ?? pruneStatusFromLive(pruneStatusLive);
  const series = buildOpsSeries(samples);

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
    series,
    pendingStall: detectPendingStall(series, 15 * 60 * 1000),
    warnings,
  };
}

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

async function tryRpc<T>(call: () => Promise<T>, warningMessage: string, warnings: string[]): Promise<T | null> {
  try {
    return await call();
  } catch {
    warnings.push(warningMessage);
    return null;
  }
}
