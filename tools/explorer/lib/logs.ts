// どこで: Logsビュー取得 / 何を: query文字列をRPC filterへ変換して結果を返す / なぜ: /logs ページの責務を単純化するため

import { isAddressHex, isTxHashHex, normalizeHex, toHexLower } from "./hex";
import { getReceiptStatusByTxHashes } from "./db";
import {
  getRpcHeadNumber,
  getRpcLogsViaGateway,
  type EthLogItemView,
  type EthJsonRpcLogsFilterInput,
} from "./rpc";

const DEFAULT_LOGS_WINDOW = 20;

export type LogsView = {
  filters: {
    fromBlock: string;
    toBlock: string;
    address: string;
    topic0: string;
    blockHash: string;
    window: string;
  };
  items: Array<{
    txHashHex: string;
    ethTxHashHex: string | null;
    blockNumber: string;
    txIndex: number;
    logIndex: number;
    addressHex: string;
    topic0Hex: string | null;
    topicsCount: number;
    dataHex: string;
    receiptStatus: number | null;
  }>;
  nextCursor: string | null;
  error: string | null;
};

export async function getLogsView(searchParams: {
  fromBlock?: string;
  toBlock?: string;
  address?: string;
  topic0?: string;
  blockHash?: string;
  window?: string;
  cursor?: string;
}): Promise<LogsView> {
  const filters: LogsView["filters"] = {
    fromBlock: searchParams.fromBlock?.trim() ?? "",
    toBlock: searchParams.toBlock?.trim() ?? "",
    address: searchParams.address?.trim() ?? "",
    topic0: searchParams.topic0?.trim() ?? "",
    blockHash: searchParams.blockHash?.trim() ?? "",
    window: searchParams.window?.trim() ?? "",
  };

  // 初期表示（入力なし）は最新Nブロックを既定検索する。
  if (!hasAnySearchInput(filters)) {
    const windowOut = parseWindowSize(filters.window);
    if (!windowOut.ok) {
      return { filters, items: [], nextCursor: null, error: windowOut.error };
    }
    const head = await getRpcHeadNumber();
    const range = buildDefaultRange(head, windowOut.window);
    filters.fromBlock = range.fromBlock;
    filters.toBlock = range.toBlock;
    if (filters.window === "") {
      filters.window = String(DEFAULT_LOGS_WINDOW);
    }
  }

  const rpcFilter = parseFilter({
    ...filters,
  });
  if (!rpcFilter.ok) {
    return { filters, items: [], nextCursor: null, error: rpcFilter.error };
  }
  try {
    const rpcItems = await getRpcLogsViaGateway(rpcFilter.filter);
    const mappedItems = rpcItems.map(mapItem);
    const statusByTxHash = await getReceiptStatusByTxHashes(mappedItems.map((item) => item.txHashHex));
    return {
      filters,
      items: mappedItems.map((item) => ({
        ...item,
        receiptStatus: statusByTxHash.get(item.txHashHex) ?? null,
      })),
      nextCursor: null,
      error: null,
    };
  } catch (error) {
    return {
      filters,
      items: [],
      nextCursor: null,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

function parseFilter(filters: {
  fromBlock: string;
  toBlock: string;
  address: string;
  topic0: string;
  blockHash: string;
}):
  | { ok: true; filter: EthJsonRpcLogsFilterInput }
  | { ok: false; error: string } {
  const out: EthJsonRpcLogsFilterInput = {};
  if (filters.blockHash !== "") {
    if (!isTxHashHex(filters.blockHash)) {
      return { ok: false, error: "blockHash must be a 32-byte hex string." };
    }
    if (filters.fromBlock !== "" || filters.toBlock !== "") {
      return { ok: false, error: "blockHash cannot be combined with fromBlock/toBlock." };
    }
    out.blockHash = normalizeHex(filters.blockHash);
  }
  if (filters.fromBlock !== "") {
    if (!/^\d+$/.test(filters.fromBlock)) {
      return { ok: false, error: "fromBlock must be an integer." };
    }
    out.fromBlock = BigInt(filters.fromBlock);
  }
  if (filters.toBlock !== "") {
    if (!/^\d+$/.test(filters.toBlock)) {
      return { ok: false, error: "toBlock must be an integer." };
    }
    out.toBlock = BigInt(filters.toBlock);
  }
  if (out.fromBlock !== undefined && out.toBlock !== undefined && out.toBlock < out.fromBlock) {
    return { ok: false, error: "toBlock must be greater than or equal to fromBlock." };
  }
  if (filters.address !== "") {
    if (!isAddressHex(filters.address)) {
      return { ok: false, error: "address must be a 20-byte hex string." };
    }
    out.address = normalizeHex(filters.address);
  }
  if (filters.topic0 !== "") {
    if (!isTxHashHex(filters.topic0)) {
      return { ok: false, error: "topic0 must be a 32-byte hex string." };
    }
    out.topic0 = normalizeHex(filters.topic0);
  }
  return { ok: true, filter: out };
}

export const logsTestHooks = {
  buildDefaultRange,
  parseWindowSize,
  parseFilter,
  hasAnySearchInput,
};

function parseWindowSize(raw: string): { ok: true; window: number } | { ok: false; error: string } {
  if (raw === "") {
    return { ok: true, window: DEFAULT_LOGS_WINDOW };
  }
  if (!/^\d+$/.test(raw)) {
    return { ok: false, error: "window must be an integer." };
  }
  const window = Number(raw);
  if (!Number.isSafeInteger(window) || window < 1 || window > 2000) {
    return { ok: false, error: "window must be in the range 1..2000." };
  }
  return { ok: true, window };
}

function buildDefaultRange(headBlock: bigint, windowSize: number): { fromBlock: string; toBlock: string } {
  const span = BigInt(windowSize - 1);
  const from = headBlock > span ? headBlock - span : 0n;
  return {
    fromBlock: from.toString(),
    toBlock: headBlock.toString(),
  };
}

function mapItem(item: EthLogItemView): {
  txHashHex: string;
  ethTxHashHex: string | null;
  blockNumber: string;
  txIndex: number;
  logIndex: number;
  addressHex: string;
  topic0Hex: string | null;
  topicsCount: number;
  dataHex: string;
  receiptStatus: number | null;
} {
  const topic0 = item.topics[0];
  return {
    txHashHex: toHexLower(item.tx_hash),
    ethTxHashHex: item.eth_tx_hash.length === 0 ? null : toHexLower(item.eth_tx_hash[0]),
    blockNumber: item.block_number.toString(),
    txIndex: item.tx_index,
    logIndex: item.log_index,
    addressHex: toHexLower(item.address),
    topic0Hex: topic0 ? toHexLower(topic0) : null,
    topicsCount: item.topics.length,
    dataHex: toHexLower(item.data),
    receiptStatus: null,
  };
}

function hasAnySearchInput(
  filters: {
  fromBlock: string;
  toBlock: string;
  address: string;
  topic0: string;
  blockHash: string;
  window: string;
  }
): boolean {
  return (
    filters.fromBlock !== "" ||
    filters.toBlock !== "" ||
    filters.address !== "" ||
    filters.topic0 !== "" ||
    filters.blockHash !== ""
  );
}
