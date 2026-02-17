// どこで: Explorerデータ層(address補助) / 何を: address履歴の変換とカーソル処理 / なぜ: data.tsを小さく保守しやすくするため

import type { AddressTxCursor, TxSummary } from "./db";
import { normalizeHex, parseHex, toHexLower } from "./hex";

export const ADDRESS_HISTORY_LIMIT = 50;

export type AddressHistoryItem = {
  txHashHex: string;
  blockNumber: bigint;
  txIndex: number;
  direction: "in" | "out" | "self";
  counterpartyHex: string | null;
  receiptStatus: number | null;
};

export type AddressView = {
  addressHex: string;
  balance: bigint | null;
  nonce: bigint | null;
  codeBytes: number | null;
  isContract: boolean | null;
  history: AddressHistoryItem[];
  failedHistory: AddressHistoryItem[];
  nextCursor: string | null;
  warnings: string[];
};

export function mapAddressHistory(rows: TxSummary[], targetHex: string): AddressHistoryItem[] {
  return rows.map((row) => toAddressHistoryItem(row, targetHex));
}

export function parseAddressCursor(cursorToken?: string): AddressTxCursor | null {
  if (!cursorToken) {
    return null;
  }
  const [block, index, txHashNoPrefix] = cursorToken.split(":");
  if (!block || !index || !txHashNoPrefix) {
    return null;
  }
  if (!/^[0-9]+$/.test(block) || !/^[0-9]+$/.test(index)) {
    return null;
  }
  const txHashHex = normalizeHex(txHashNoPrefix);
  if (!/^0x[0-9a-f]{64}$/.test(txHashHex)) {
    return null;
  }
  const txIndex = Number(index);
  if (!Number.isSafeInteger(txIndex) || txIndex < 0) {
    return null;
  }
  return {
    blockNumber: BigInt(block),
    txIndex,
    txHash: parseHex(txHashHex),
  };
}

export function buildAddressCursor(tx: TxSummary): string {
  return `${tx.blockNumber.toString()}:${tx.txIndex}:${tx.txHashHex.slice(2)}`;
}

function toAddressHistoryItem(tx: TxSummary, targetHex: string): AddressHistoryItem {
  const fromHex = toHexLower(tx.fromAddress);
  const toHex = tx.toAddress ? toHexLower(tx.toAddress) : null;
  if (fromHex === targetHex && toHex === targetHex) {
    return {
      txHashHex: tx.txHashHex,
      blockNumber: tx.blockNumber,
      txIndex: tx.txIndex,
      direction: "self",
      counterpartyHex: targetHex,
      receiptStatus: tx.receiptStatus,
    };
  }
  if (fromHex === targetHex) {
    return {
      txHashHex: tx.txHashHex,
      blockNumber: tx.blockNumber,
      txIndex: tx.txIndex,
      direction: "out",
      counterpartyHex: toHex,
      receiptStatus: tx.receiptStatus,
    };
  }
  return {
    txHashHex: tx.txHashHex,
    blockNumber: tx.blockNumber,
    txIndex: tx.txIndex,
    direction: "in",
    counterpartyHex: fromHex,
    receiptStatus: tx.receiptStatus,
  };
}
