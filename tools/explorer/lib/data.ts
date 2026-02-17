// どこで: Explorerデータ取得層 / 何を: PostgresとRPCの問い合わせをユースケース単位で束ねる / なぜ: page側の分岐ロジックを最小化するため

import { Principal } from "@dfinity/principal";
import { loadConfig } from "./config";
import { extractErc20TransfersFromReceipt, type Erc20TransferView } from "./erc20";
import { getTokenMeta } from "./token_meta";
import {
  getBlockDetails,
  getLatestBlocks,
  getLatestTxs,
  getMaxBlockNumber,
  getMetaSnapshot,
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
  buildOpsSeries,
  detectPendingStall,
  parseStoredPruneStatus,
  pruneStatusFromLive,
  type OpsSeriesPoint,
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

export type TxDetailView = {
  tx: TxSummaryWithPrincipal;
  statusLabel: string;
  valueWei: bigint | null;
  gasPriceWei: bigint | null;
  transactionFeeWei: bigint | null;
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
  const gasPriceWei = rpcTx?.decoded[0] ? rpcTx.decoded[0].gas_price : null;
  const transactionFeeWei = "Ok" in receiptOut ? receiptOut.Ok.total_fee : null;
  const erc20TransfersRaw = "Ok" in receiptOut ? extractErc20TransfersFromReceipt(receiptOut.Ok) : [];
  const erc20Transfers = await withTokenMeta(erc20TransfersRaw);
  return {
    tx,
    statusLabel: receiptStatusLabel(tx.receiptStatus),
    valueWei,
    gasPriceWei,
    transactionFeeWei,
    erc20Transfers,
  };
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
