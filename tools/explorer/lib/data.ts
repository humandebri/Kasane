// どこで: Explorerデータ取得層 / 何を: RPC問い合わせをユースケース単位で束ねる / なぜ: SQLite依存を排除して運用を単純化するため

import { loadConfig } from "./config";
import { parseHex, toHexLower } from "./hex";
import { getReceiptByTxId, getRpcBlockWithTxMode, getRpcHeadNumber, type ReceiptView, type LookupError } from "./rpc";

type HomeView = {
  rpcHead: bigint;
  blocks: BlockSummary[];
  txs: TxSummary[];
};

export type BlockSummary = {
  number: bigint;
  hashHex: string;
  timestamp: bigint;
  txCount: number;
};

export type TxSummary = {
  txHashHex: string;
  blockNumber: bigint;
  txIndex: number;
};

export type BlockDetails = {
  block: BlockSummary;
  txs: TxSummary[];
};

export async function getHomeView(): Promise<HomeView> {
  const cfg = loadConfig(process.env);
  const rpcHead = await getRpcHeadNumber();
  const blockNumbers: bigint[] = [];
  for (let i = 0; i < cfg.latestBlocksLimit; i += 1) {
    const n = rpcHead - BigInt(i);
    if (n < 0n) {
      break;
    }
    blockNumbers.push(n);
  }

  const rpcBlocks = await Promise.all(blockNumbers.map((n) => getRpcBlockWithTxMode(n, true)));
  const blocks: BlockSummary[] = [];
  const txs: TxSummary[] = [];

  for (const rpcBlock of rpcBlocks) {
    if (!rpcBlock) {
      continue;
    }
    blocks.push({
      number: rpcBlock.number,
      hashHex: toHexLower(rpcBlock.block_hash),
      timestamp: rpcBlock.timestamp,
      txCount: "Full" in rpcBlock.txs ? rpcBlock.txs.Full.length : rpcBlock.txs.Hashes.length,
    });

    if ("Full" in rpcBlock.txs) {
      rpcBlock.txs.Full.forEach((tx, txIndex) => {
        if (tx.eth_tx_hash.length === 0) {
          return;
        }
        txs.push({
          txHashHex: toHexLower(tx.eth_tx_hash[0]),
          blockNumber: rpcBlock.number,
          txIndex,
        });
      });
    } else {
      rpcBlock.txs.Hashes.forEach((txHash, txIndex) => {
        txs.push({
          txHashHex: toHexLower(txHash),
          blockNumber: rpcBlock.number,
          txIndex,
        });
      });
    }
  }

  return { rpcHead, blocks, txs: txs.slice(0, cfg.latestTxsLimit) };
}

export async function getBlockView(blockNumber: bigint): Promise<BlockDetails | null> {
  const rpcBlock = await getRpcBlockWithTxMode(blockNumber, true);
  if (!rpcBlock) {
    return null;
  }

  const txs: TxSummary[] = [];
  if ("Full" in rpcBlock.txs) {
    rpcBlock.txs.Full.forEach((tx, txIndex) => {
      if (tx.eth_tx_hash.length === 0) {
        return;
      }
      txs.push({
        txHashHex: toHexLower(tx.eth_tx_hash[0]),
        blockNumber: rpcBlock.number,
        txIndex,
      });
    });
  } else {
    rpcBlock.txs.Hashes.forEach((txHash, txIndex) => {
      txs.push({
        txHashHex: toHexLower(txHash),
        blockNumber: rpcBlock.number,
        txIndex,
      });
    });
  }

  return {
    block: {
      number: rpcBlock.number,
      hashHex: toHexLower(rpcBlock.block_hash),
      timestamp: rpcBlock.timestamp,
      txCount: txs.length,
    },
    txs,
  };
}

export async function getTxView(txHashHex: string): Promise<TxSummary | null> {
  const txHash = parseHex(txHashHex);
  const out = await getReceiptByTxId(txHash);
  if ("Err" in out) {
    return null;
  }
  return {
    txHashHex: toHexLower(out.Ok.tx_id),
    blockNumber: out.Ok.block_number,
    txIndex: out.Ok.tx_index,
  };
}

export async function getReceiptView(
  txHashHex: string
): Promise<{ tx: TxSummary | null; receipt: ReceiptView | null; lookupError: LookupError | null; txExistsInIndex: boolean }> {
  const txHash = parseHex(txHashHex);
  const out = await getReceiptByTxId(txHash);
  if ("Ok" in out) {
    const tx: TxSummary = {
      txHashHex: toHexLower(out.Ok.tx_id),
      blockNumber: out.Ok.block_number,
      txIndex: out.Ok.tx_index,
    };
    return { tx, receipt: out.Ok, lookupError: null, txExistsInIndex: true };
  }
  return { tx: null, receipt: null, lookupError: out.Err, txExistsInIndex: false };
}
