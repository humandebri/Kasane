// どこで: Explorerデータ取得層 / 何を: PostgresとRPCの問い合わせをユースケース単位で束ねる / なぜ: page側の分岐ロジックを最小化するため

import { Principal } from "@dfinity/principal";
import { loadConfig } from "./config";
import {
  getBlockDetails,
  getLatestBlocks,
  getLatestTxs,
  getMaxBlockNumber,
  getOverviewStats,
  getTx,
  type BlockSummary,
  type OverviewStats,
  type TxSummary,
} from "./db";
import { parseHex } from "./hex";
import {
  getReceiptByTxId,
  getRpcBlock,
  getRpcHeadNumber,
  type LookupError,
  type ReceiptView,
} from "./rpc";

type HomeView = {
  dbHead: bigint | null;
  rpcHead: bigint;
  stats: OverviewStats;
  blocks: BlockSummary[];
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
