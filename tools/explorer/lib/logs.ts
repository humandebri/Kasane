// どこで: Logsビュー取得 / 何を: query文字列をRPC filterへ変換して結果を返す / なぜ: /logs ページの責務を単純化するため

import { isAddressHex, isTxHashHex, normalizeHex, parseAddressHex, parseHex, toHexLower } from "./hex";
import {
  getRpcLogsPaged,
  type EthLogItemView,
  type EthLogFilterInput,
  type EthLogsCursorView,
  type GetLogsErrorView,
} from "./rpc";

export type LogsView = {
  filters: {
    fromBlock: string;
    toBlock: string;
    address: string;
    topic0: string;
    limit: string;
  };
  unsupportedNotes: string[];
  items: Array<{
    txHashHex: string;
    ethTxHashHex: string | null;
    blockNumber: bigint;
    txIndex: number;
    logIndex: number;
    addressHex: string;
    topic0Hex: string | null;
    topicsCount: number;
    dataHex: string;
  }>;
  nextCursor: string | null;
  error: string | null;
};

export async function getLogsView(searchParams: {
  fromBlock?: string;
  toBlock?: string;
  address?: string;
  topic0?: string;
  topic1?: string;
  blockHash?: string;
  limit?: string;
  cursor?: string;
}): Promise<LogsView> {
  const filters = {
    fromBlock: searchParams.fromBlock?.trim() ?? "",
    toBlock: searchParams.toBlock?.trim() ?? "",
    address: searchParams.address?.trim() ?? "",
    topic0: searchParams.topic0?.trim() ?? "",
    limit: searchParams.limit?.trim() ?? "200",
  };
  const unsupportedNotes = [
    "topic1 はcanister未対応です（topic0のみ対応）",
    "topics OR配列は未対応です",
    "blockHashフィルタは未対応です",
    "from/to の block span 上限は 1000 です",
    "page limit 上限は 2000 です",
  ];

  const rpcFilter = parseFilter({
    ...filters,
    topic1: searchParams.topic1?.trim() ?? "",
    blockHash: searchParams.blockHash?.trim() ?? "",
  });
  const cursor = parseCursor(searchParams.cursor);
  if (!rpcFilter.ok) {
    return { filters, unsupportedNotes, items: [], nextCursor: null, error: rpcFilter.error };
  }
  const out = await getRpcLogsPaged(rpcFilter.filter, cursor, rpcFilter.pageLimit);
  if ("Err" in out) {
    return {
      filters,
      unsupportedNotes,
      items: [],
      nextCursor: null,
      error: toErrorText(out.Err),
    };
  }
  return {
    filters,
    unsupportedNotes,
    items: out.Ok.items.map(mapItem),
    nextCursor: out.Ok.next_cursor.length === 0 ? null : encodeCursor(out.Ok.next_cursor[0]),
    error: null,
  };
}

function parseFilter(filters: {
  fromBlock: string;
  toBlock: string;
  address: string;
  topic0: string;
  topic1: string;
  blockHash: string;
  limit: string;
}):
  | { ok: true; filter: EthLogFilterInput; pageLimit: number }
  | { ok: false; error: string } {
  if (filters.topic1 !== "") {
    return { ok: false, error: "topic1 は未対応です（topic0 のみ指定してください）" };
  }
  if (filters.blockHash !== "") {
    return { ok: false, error: "blockHash フィルタは未対応です（from/to を使用してください）" };
  }
  const out: EthLogFilterInput = {};
  if (filters.fromBlock !== "") {
    if (!/^\d+$/.test(filters.fromBlock)) {
      return { ok: false, error: "fromBlock は整数で指定してください" };
    }
    out.fromBlock = BigInt(filters.fromBlock);
  }
  if (filters.toBlock !== "") {
    if (!/^\d+$/.test(filters.toBlock)) {
      return { ok: false, error: "toBlock は整数で指定してください" };
    }
    out.toBlock = BigInt(filters.toBlock);
  }
  if (out.fromBlock !== undefined && out.toBlock !== undefined && out.toBlock < out.fromBlock) {
    return { ok: false, error: "toBlock は fromBlock 以上にしてください" };
  }
  if (filters.address !== "") {
    if (!isAddressHex(filters.address)) {
      return { ok: false, error: "address は 20-byte hex で指定してください" };
    }
    out.address = parseAddressHex(normalizeHex(filters.address));
  }
  if (filters.topic0 !== "") {
    if (!isTxHashHex(filters.topic0)) {
      return { ok: false, error: "topic0 は 32-byte hex で指定してください" };
    }
    out.topic0 = parseHex(normalizeHex(filters.topic0));
  }
  let pageLimit = 200;
  if (filters.limit !== "") {
    if (!/^\d+$/.test(filters.limit)) {
      return { ok: false, error: "limit は整数で指定してください" };
    }
    const parsed = Number(filters.limit);
    if (!Number.isSafeInteger(parsed) || parsed < 1 || parsed > 2000) {
      return { ok: false, error: "limit は 1..2000 で指定してください" };
    }
    pageLimit = parsed;
  }
  return { ok: true, filter: out, pageLimit };
}

export const logsTestHooks = {
  parseFilter,
};

function parseCursor(raw?: string): EthLogsCursorView | null {
  if (!raw) {
    return null;
  }
  const [block, txIndex, logIndex] = raw.split(":");
  if (!block || !txIndex || !logIndex) {
    return null;
  }
  if (!/^\d+$/.test(block) || !/^\d+$/.test(txIndex) || !/^\d+$/.test(logIndex)) {
    return null;
  }
  return {
    block_number: BigInt(block),
    tx_index: Number(txIndex),
    log_index: Number(logIndex),
  };
}

function encodeCursor(cursor?: EthLogsCursorView): string | null {
  if (!cursor) {
    return null;
  }
  return `${cursor.block_number.toString()}:${cursor.tx_index}:${cursor.log_index}`;
}

function mapItem(item: EthLogItemView): {
  txHashHex: string;
  ethTxHashHex: string | null;
  blockNumber: bigint;
  txIndex: number;
  logIndex: number;
  addressHex: string;
  topic0Hex: string | null;
  topicsCount: number;
  dataHex: string;
} {
  const topic0 = item.topics[0];
  return {
    txHashHex: toHexLower(item.tx_hash),
    ethTxHashHex: item.eth_tx_hash.length === 0 ? null : toHexLower(item.eth_tx_hash[0]),
    blockNumber: item.block_number,
    txIndex: item.tx_index,
    logIndex: item.log_index,
    addressHex: toHexLower(item.address),
    topic0Hex: topic0 ? toHexLower(topic0) : null,
    topicsCount: item.topics.length,
    dataHex: toHexLower(item.data),
  };
}

function toErrorText(error: GetLogsErrorView): string {
  if ("TooManyResults" in error) {
    return "TooManyResults: limitを下げるか範囲を絞ってください";
  }
  if ("RangeTooLarge" in error) {
    return "RangeTooLarge: from/to の範囲を縮めてください";
  }
  if ("InvalidArgument" in error) {
    return `InvalidArgument: ${error.InvalidArgument}`;
  }
  return `UnsupportedFilter: ${error.UnsupportedFilter}`;
}
