// どこで: Explorerデータ層(address補助) / 何を: address履歴の変換とカーソル処理 / なぜ: data.tsを小さく保守しやすくするため

import type {
  AddressTokenTransferCursor,
  AddressTxCursor,
  TokenTransferSummary,
  TxSummary,
} from "./db";
import type { TokenMetaView } from "./token_meta";
import { normalizeHex, parseHex, toHexLower } from "./hex";
import { formatTokenAmount } from "./format";
import { inferMethodLabel } from "./tx_method";

export const ADDRESS_HISTORY_LIMIT = 50;

export type AddressHistoryItem = {
  txHashHex: string;
  blockNumber: bigint;
  blockTimestamp: bigint | null;
  txIndex: number;
  fromAddressHex: string;
  toAddressHex: string | null;
  createdContractAddressHex: string | null;
  txSelectorHex: string | null;
  methodLabel: string;
  direction: "in" | "out" | "self";
  counterpartyHex: string | null;
  receiptStatus: number | null;
};

export type AddressTokenTransferItem = {
  txHashHex: string;
  blockNumber: bigint;
  blockTimestamp: bigint | null;
  txIndex: number;
  logIndex: number;
  receiptStatus: number | null;
  txSelectorHex: string | null;
  methodLabel: string;
  tokenAddressHex: string;
  fromAddressHex: string;
  toAddressHex: string;
  direction: "in" | "out" | "self";
  amount: bigint;
  amountText: string;
  tokenLabel: string;
};

export type AddressView = {
  addressHex: string;
  providedPrincipal: string | null;
  submitterPrincipals: string[];
  balance: bigint | null;
  nonce: bigint | null;
  codeBytes: number | null;
  isContract: boolean | null;
  history: AddressHistoryItem[];
  failedHistory: AddressHistoryItem[];
  nextCursor: string | null;
  tokenTransfers: AddressTokenTransferItem[];
  tokenNextCursor: string | null;
  warnings: string[];
  erc20Meta: {
    name: string;
    symbol: string;
    decimals: number;
    totalSupplyRaw: bigint;
    totalSupplyFormatted: string;
  } | null;
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

export function parseTokenTransferCursor(cursorToken?: string): AddressTokenTransferCursor | null {
  if (!cursorToken) {
    return null;
  }
  const [block, txIndexText, logIndexText, txHashNoPrefix] = cursorToken.split(":");
  if (!block || !txIndexText || !logIndexText || !txHashNoPrefix) {
    return null;
  }
  if (!/^[0-9]+$/.test(block) || !/^[0-9]+$/.test(txIndexText) || !/^[0-9]+$/.test(logIndexText)) {
    return null;
  }
  const txHashHex = normalizeHex(txHashNoPrefix);
  if (!/^0x[0-9a-f]{64}$/.test(txHashHex)) {
    return null;
  }
  const txIndex = Number(txIndexText);
  const logIndex = Number(logIndexText);
  if (!Number.isSafeInteger(txIndex) || txIndex < 0 || !Number.isSafeInteger(logIndex) || logIndex < 0) {
    return null;
  }
  return {
    blockNumber: BigInt(block),
    txIndex,
    logIndex,
    txHash: parseHex(txHashHex),
  };
}

export function buildTokenTransferCursor(item: TokenTransferSummary): string {
  return `${item.blockNumber.toString()}:${item.txIndex}:${item.logIndex}:${item.txHashHex.slice(2)}`;
}

export function mapAddressTokenTransfers(
  rows: TokenTransferSummary[],
  targetHex: string,
  metaByTokenHex: ReadonlyMap<string, TokenMetaView>,
  targetTokenLabel: string | null = null
): AddressTokenTransferItem[] {
  return rows.map((row) => {
    const fromAddressHex = toHexLower(row.fromAddress);
    const toAddressHex = toHexLower(row.toAddress);
    const tokenAddressHex = toHexLower(row.tokenAddress);
    const tokenMeta = metaByTokenHex.get(tokenAddressHex) ?? { symbol: null, decimals: null };
    const direction = fromAddressHex === targetHex && toAddressHex === targetHex
      ? "self"
      : fromAddressHex === targetHex
        ? "out"
        : "in";
    return {
      txHashHex: row.txHashHex,
      blockNumber: row.blockNumber,
      blockTimestamp: row.blockTimestamp ?? null,
      txIndex: row.txIndex,
      logIndex: row.logIndex,
      receiptStatus: row.receiptStatus,
      txSelectorHex: row.txSelector ? toHexLower(row.txSelector) : null,
      methodLabel: inferMethodLabel(tokenAddressHex, row.txSelector),
      tokenAddressHex,
      fromAddressHex,
      toAddressHex,
      direction,
      amount: row.amount,
      amountText: formatTokenAmount(row.amount, tokenMeta.decimals),
      tokenLabel:
        tokenAddressHex === targetHex && targetTokenLabel
          ? targetTokenLabel
          : (tokenMeta.symbol ?? shortAddressLabel(tokenAddressHex)),
    };
  });
}

function toAddressHistoryItem(tx: TxSummary, targetHex: string): AddressHistoryItem {
  const fromHex = toHexLower(tx.fromAddress);
  const toHex = tx.toAddress ? toHexLower(tx.toAddress) : null;
  const createdContractAddressHex = tx.createdContractAddress ? toHexLower(tx.createdContractAddress) : null;
  if (fromHex === targetHex && toHex === targetHex) {
    return {
      txHashHex: tx.txHashHex,
      blockNumber: tx.blockNumber,
      blockTimestamp: tx.blockTimestamp ?? null,
      txIndex: tx.txIndex,
      fromAddressHex: fromHex,
      toAddressHex: toHex,
      createdContractAddressHex,
      txSelectorHex: toHex === null || !tx.txSelector ? null : toHexLower(tx.txSelector),
      methodLabel: inferMethodLabel(toHex, tx.txSelector),
      direction: "self",
      counterpartyHex: targetHex,
      receiptStatus: tx.receiptStatus,
    };
  }
  if (fromHex === targetHex) {
    return {
      txHashHex: tx.txHashHex,
      blockNumber: tx.blockNumber,
      blockTimestamp: tx.blockTimestamp ?? null,
      txIndex: tx.txIndex,
      fromAddressHex: fromHex,
      toAddressHex: toHex,
      createdContractAddressHex,
      txSelectorHex: toHex === null || !tx.txSelector ? null : toHexLower(tx.txSelector),
      methodLabel: inferMethodLabel(toHex, tx.txSelector),
      direction: "out",
      counterpartyHex: toHex,
      receiptStatus: tx.receiptStatus,
    };
  }
  return {
    txHashHex: tx.txHashHex,
    blockNumber: tx.blockNumber,
    blockTimestamp: tx.blockTimestamp ?? null,
    txIndex: tx.txIndex,
    fromAddressHex: fromHex,
    toAddressHex: toHex,
    createdContractAddressHex,
    txSelectorHex: toHex === null || !tx.txSelector ? null : toHexLower(tx.txSelector),
    methodLabel: inferMethodLabel(toHex, tx.txSelector),
    direction: "in",
    counterpartyHex: fromHex,
    receiptStatus: tx.receiptStatus,
  };
}

function shortAddressLabel(addressHex: string): string {
  if (addressHex.length <= 12) {
    return addressHex;
  }
  return `${addressHex.slice(0, 8)}...${addressHex.slice(-4)}`;
}
