// どこで: Logsビュー取得 / 何を: query文字列をRPC filterへ変換して結果を返す / なぜ: /logs ページの責務を単純化するため

import { isAddressHex, isTxHashHex, normalizeHex, parseAddressHex, parseHex, toHexLower } from "./hex";
import {
  getRpcHeadNumber,
  getRpcLogsPaged,
  type EthLogItemView,
  type EthLogFilterInput,
  type EthLogsCursorView,
  type GetLogsErrorView,
} from "./rpc";

const DEFAULT_LOGS_WINDOW = 20;
const DEFAULT_LOGS_PAGE_LIMIT = 100;

export type LogsView = {
  filters: {
    fromBlock: string;
    toBlock: string;
    address: string;
    topic0: string;
    window: string;
  };
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
  blockHash?: string;
  window?: string;
  cursor?: string;
}): Promise<LogsView> {
  const filters: LogsView["filters"] = {
    fromBlock: searchParams.fromBlock?.trim() ?? "",
    toBlock: searchParams.toBlock?.trim() ?? "",
    address: searchParams.address?.trim() ?? "",
    topic0: searchParams.topic0?.trim() ?? "",
    window: searchParams.window?.trim() ?? "",
  };
  const cursor = parseCursor(searchParams.cursor);

  // 初期表示（入力なし）は最新Nブロックを既定検索する。
  if (!cursor && !hasAnySearchInput(filters, searchParams.blockHash?.trim() ?? "")) {
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
    blockHash: searchParams.blockHash?.trim() ?? "",
  });
  if (!rpcFilter.ok) {
    return { filters, items: [], nextCursor: null, error: rpcFilter.error };
  }
  const out = await getRpcLogsPaged(rpcFilter.filter, cursor, rpcFilter.pageLimit);
  if ("Err" in out) {
    return {
      filters,
      items: [],
      nextCursor: null,
      error: toErrorText(out.Err),
    };
  }
  return {
    filters,
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
  blockHash: string;
}):
  | { ok: true; filter: EthLogFilterInput; pageLimit: number }
  | { ok: false; error: string } {
  if (filters.blockHash !== "") {
    return { ok: false, error: "blockHash filter is not supported. Use fromBlock/toBlock." };
  }
  const out: EthLogFilterInput = {};
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
    out.address = parseAddressHex(normalizeHex(filters.address));
  }
  if (filters.topic0 !== "") {
    if (!isTxHashHex(filters.topic0)) {
      return { ok: false, error: "topic0 must be a 32-byte hex string." };
    }
    out.topic0 = parseHex(normalizeHex(filters.topic0));
  }
  return { ok: true, filter: out, pageLimit: DEFAULT_LOGS_PAGE_LIMIT };
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
    return "TooManyResults: narrow the block range.";
  }
  if ("RangeTooLarge" in error) {
    return "RangeTooLarge: reduce the fromBlock/toBlock span.";
  }
  if ("InvalidArgument" in error) {
    return `InvalidArgument: ${error.InvalidArgument}`;
  }
  return `UnsupportedFilter: ${error.UnsupportedFilter}`;
}

function hasAnySearchInput(
  filters: {
  fromBlock: string;
  toBlock: string;
  address: string;
  topic0: string;
  window: string;
  },
  blockHash: string = ""
): boolean {
  return (
    filters.fromBlock !== "" ||
    filters.toBlock !== "" ||
    filters.address !== "" ||
    filters.topic0 !== "" ||
    blockHash !== ""
  );
}
