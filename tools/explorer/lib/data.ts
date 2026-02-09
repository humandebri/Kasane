// どこで: Explorerデータ取得層 / 何を: DBとRPCの問い合わせをユースケース単位で束ねる / なぜ: page側の分岐ロジックを最小化するため

import { loadConfig } from "./config";
import { ExplorerDb, type BlockDetails, type BlockSummary, type TxSummary } from "./db";
import { parseHex } from "./hex";
import { getReceiptByTxId, getRpcBlock, getRpcHeadNumber, type ReceiptView, type LookupError } from "./rpc";

type HomeView = {
  dbHead: bigint | null;
  rpcHead: bigint;
  blocks: BlockSummary[];
  txs: TxSummary[];
};

function withDb<T>(fn: (db: ExplorerDb) => T): T {
  const cfg = loadConfig(process.env);
  const db = new ExplorerDb(cfg.dbPath);
  try {
    return fn(db);
  } finally {
    db.close();
  }
}

export async function getHomeView(): Promise<HomeView> {
  const cfg = loadConfig(process.env);
  const rpcHead = await getRpcHeadNumber();
  const local = withDb((db) => ({
    dbHead: db.getMaxBlockNumber(),
    blocks: db.getLatestBlocks(cfg.latestBlocksLimit),
    txs: db.getLatestTxs(cfg.latestTxsLimit),
  }));
  return { rpcHead, ...local };
}

export async function getBlockView(blockNumber: bigint): Promise<{ db: BlockDetails | null; rpcExists: boolean }> {
  const db = withDb((inner) => inner.getBlockDetails(blockNumber));
  const rpcBlock = await getRpcBlock(blockNumber);
  return { db, rpcExists: rpcBlock !== null };
}

export function getTxView(txHashHex: string): TxSummary | null {
  const txHash = parseHex(txHashHex);
  return withDb((db) => db.getTx(txHash));
}

export async function getReceiptView(
  txHashHex: string
): Promise<{ tx: TxSummary | null; receipt: ReceiptView | null; lookupError: LookupError | null }> {
  const txHash = parseHex(txHashHex);
  const tx = withDb((db) => db.getTx(txHash));
  const out = await getReceiptByTxId(txHash);
  if ("Ok" in out) {
    return { tx, receipt: out.Ok, lookupError: null };
  }
  return { tx, receipt: null, lookupError: out.Err };
}
