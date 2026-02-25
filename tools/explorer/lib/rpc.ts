// どこで: Explorer RPC層 / 何を: canister query を型付きで実行 / なぜ: DB外のライブ情報を取得するため

import { Actor, HttpAgent } from "@dfinity/agent";
import type { IDL } from "@dfinity/candid";
import { loadConfig } from "./config";
import { bytesToBigInt, normalizeHex, parseHex } from "./hex";

export type LookupError = { NotFound: null } | { Pending: null } | { Pruned: { pruned_before_block: bigint } };

type Result<T, E> = { Ok: T } | { Err: E };
type ResultBytes = Result<Uint8Array, string>;
type ResultNonce = Result<bigint, string>;
type ResultRpcCall = Result<RpcCallResultView, RpcErrorView>;
type ResultRpcBytes = Result<Uint8Array, RpcErrorView>;

type RpcBlockTagView =
  | { Latest: null }
  | { Pending: null }
  | { Safe: null }
  | { Finalized: null }
  | { Earliest: null }
  | { Number: bigint };

export type LogView = { address: Uint8Array; topics: Uint8Array[]; data: Uint8Array };
export type ReceiptView = {
  tx_id: Uint8Array;
  block_number: bigint;
  tx_index: number;
  status: number;
  gas_used: bigint;
  effective_gas_price: bigint;
  l1_data_fee: bigint;
  operator_fee: bigint;
  total_fee: bigint;
  contract_address: [] | [Uint8Array];
  return_data_hash: Uint8Array;
  return_data: [] | [Uint8Array];
  logs: LogView[];
};

export type TxKindView = { EthSigned: null } | { IcSynthetic: null };
export type RpcTxDecodedView = {
  from: Uint8Array;
  to: [] | [Uint8Array];
  value: Uint8Array;
  input: Uint8Array;
  nonce: bigint;
  gas_limit: bigint;
  gas_price: [] | [bigint];
  max_fee_per_gas: [] | [bigint];
  max_priority_fee_per_gas: [] | [bigint];
  chain_id: [] | [bigint];
};
export type RpcTxView = {
  kind: TxKindView;
  hash: Uint8Array;
  tx_index: [] | [number];
  block_number: [] | [bigint];
  eth_tx_hash: [] | [Uint8Array];
  caller_principal: [] | [Uint8Array];
  decode_ok: boolean;
  decoded: [] | [RpcTxDecodedView];
  raw: Uint8Array;
};

export type EthBlockView = {
  block_hash: Uint8Array;
  parent_hash: Uint8Array;
  state_root: Uint8Array;
  number: bigint;
  timestamp: bigint;
  beneficiary: Uint8Array;
  gas_limit: [] | [bigint];
  gas_used: [] | [bigint];
  base_fee_per_gas: [] | [bigint];
  txs: { Full: RpcTxView[] } | { Hashes: Uint8Array[] };
};

export type PruneStatusView = {
  pruning_enabled: boolean;
  prune_running: boolean;
  estimated_kept_bytes: bigint;
  high_water_bytes: bigint;
  low_water_bytes: bigint;
  hard_emergency_bytes: bigint;
  last_prune_at: bigint;
  pruned_before_block: [] | [bigint];
  oldest_kept_block: [] | [bigint];
  oldest_kept_timestamp: [] | [bigint];
  need_prune: boolean;
};

export type EthLogFilterInput = {
  fromBlock?: bigint;
  toBlock?: bigint;
  address?: Uint8Array;
  topic0?: Uint8Array;
  topic1?: Uint8Array;
  limit?: number;
};

export type EthJsonRpcLogsFilterInput = {
  fromBlock?: bigint;
  toBlock?: bigint;
  address?: string;
  topic0?: string;
  blockHash?: string;
};

export type EthLogsCursorView = {
  block_number: bigint;
  tx_index: number;
  log_index: number;
};

export type EthLogItemView = {
  tx_hash: Uint8Array;
  eth_tx_hash: [] | [Uint8Array];
  block_number: bigint;
  tx_index: number;
  log_index: number;
  address: Uint8Array;
  topics: Uint8Array[];
  data: Uint8Array;
};

export type EthLogsPageView = {
  items: EthLogItemView[];
  next_cursor: [] | [EthLogsCursorView];
};

export type GetLogsErrorView =
  | { TooManyResults: null }
  | { RangeTooLarge: null }
  | { InvalidArgument: string }
  | { UnsupportedFilter: string };

export type RpcReceiptLookupView =
  | { NotFound: null }
  | { Found: EthReceiptView }
  | { PossiblyPruned: { pruned_before_block: bigint } }
  | { Pruned: { pruned_before_block: bigint } };

export type EthReceiptView = {
  tx_hash: Uint8Array;
  eth_tx_hash: [] | [Uint8Array];
  block_number: bigint;
  tx_index: number;
  status: number;
  gas_used: bigint;
  effective_gas_price: bigint;
  l1_data_fee: bigint;
  operator_fee: bigint;
  total_fee: bigint;
  contract_address: [] | [Uint8Array];
  logs: Array<{ log_index: number; address: Uint8Array; topics: Uint8Array[]; data: Uint8Array }>;
};

export type RpcAccessListItemView = {
  address: Uint8Array;
  storage_keys: Uint8Array[];
};

export type RpcCallObjectView = {
  to: [] | [Uint8Array];
  gas: [] | [bigint];
  value: [] | [Uint8Array];
  max_priority_fee_per_gas: [] | [bigint];
  data: [] | [Uint8Array];
  from: [] | [Uint8Array];
  max_fee_per_gas: [] | [bigint];
  chain_id: [] | [bigint];
  nonce: [] | [bigint];
  tx_type: [] | [bigint];
  access_list: [] | [RpcAccessListItemView[]];
  gas_price: [] | [bigint];
};

export type RpcCallResultView = {
  status: number;
  return_data: Uint8Array;
  gas_used: bigint;
  revert_data: [] | [Uint8Array];
};

export type RpcErrorView = {
  code: number;
  message: string;
};

export type PendingStatusView =
  | { Queued: { seq: bigint } }
  | { Included: { block_number: bigint; tx_index: number } }
  | { Dropped: { code: number } }
  | { Unknown: null };

type ExplorerActorMethods = {
  rpc_eth_block_number: () => Promise<bigint>;
  get_receipt: (txId: Uint8Array) => Promise<Result<ReceiptView, LookupError>>;
  rpc_eth_get_block_by_number: (number: bigint, fullTx: boolean) => Promise<[] | [EthBlockView]>;
  rpc_eth_get_transaction_by_tx_id: (txId: Uint8Array) => Promise<[] | [RpcTxView]>;
  rpc_eth_get_transaction_by_eth_hash: (ethTxHash: Uint8Array) => Promise<[] | [RpcTxView]>;
  rpc_eth_get_transaction_receipt_with_status_by_eth_hash: (ethTxHash: Uint8Array) => Promise<RpcReceiptLookupView>;
  rpc_eth_get_transaction_receipt_with_status_by_tx_id: (txId: Uint8Array) => Promise<RpcReceiptLookupView>;
  rpc_eth_get_logs_paged: (
    filter: {
      from_block: [] | [bigint];
      to_block: [] | [bigint];
      address: [] | [Uint8Array];
      topic0: [] | [Uint8Array];
      topic1: [] | [Uint8Array];
      limit: [] | [number];
    },
    cursor: [] | [EthLogsCursorView],
    limit: number
  ) => Promise<Result<EthLogsPageView, GetLogsErrorView>>;
  get_pending: (txId: Uint8Array) => Promise<PendingStatusView>;
  get_prune_status: () => Promise<PruneStatusView>;
  rpc_eth_get_balance: (address: Uint8Array, tag: RpcBlockTagView) => Promise<ResultRpcBytes>;
  rpc_eth_get_code: (address: Uint8Array, tag: RpcBlockTagView) => Promise<ResultRpcBytes>;
  expected_nonce_by_address: (address: Uint8Array) => Promise<ResultNonce>;
  rpc_eth_call_object: (call: RpcCallObjectView) => Promise<ResultRpcCall>;
};

let cachedActor: ExplorerActorMethods | null = null;

export async function getRpcHeadNumber(): Promise<bigint> {
  return (await getActor()).rpc_eth_block_number();
}

export async function getReceiptByTxId(txId: Uint8Array): Promise<Result<ReceiptView, LookupError>> {
  return (await getActor()).get_receipt(txId);
}

export async function getRpcBlock(number: bigint): Promise<EthBlockView | null> {
  return getRpcBlockWithTxMode(number, false);
}

export async function getRpcBlockWithTxMode(number: bigint, fullTx: boolean): Promise<EthBlockView | null> {
  const out = await (await getActor()).rpc_eth_get_block_by_number(number, fullTx);
  return out.length === 0 ? null : out[0];
}

export async function getRpcTxByTxId(txId: Uint8Array): Promise<RpcTxView | null> {
  const out = await (await getActor()).rpc_eth_get_transaction_by_tx_id(txId);
  return out.length === 0 ? null : out[0];
}

export async function getRpcTxByEthHash(ethTxHash: Uint8Array): Promise<RpcTxView | null> {
  const out = await (await getActor()).rpc_eth_get_transaction_by_eth_hash(ethTxHash);
  return out.length === 0 ? null : out[0];
}

export async function getRpcReceiptWithStatusByEthHash(ethTxHash: Uint8Array): Promise<RpcReceiptLookupView> {
  return (await getActor()).rpc_eth_get_transaction_receipt_with_status_by_eth_hash(ethTxHash);
}

export async function getRpcReceiptWithStatusByTxId(txId: Uint8Array): Promise<RpcReceiptLookupView> {
  return (await getActor()).rpc_eth_get_transaction_receipt_with_status_by_tx_id(txId);
}

export async function getRpcLogsPaged(
  filter: EthLogFilterInput,
  cursor: EthLogsCursorView | null,
  limit: number
): Promise<Result<EthLogsPageView, GetLogsErrorView>> {
  const actor = await getActor();
  return actor.rpc_eth_get_logs_paged(
    {
      from_block: filter.fromBlock === undefined ? [] : [filter.fromBlock],
      to_block: filter.toBlock === undefined ? [] : [filter.toBlock],
      address: filter.address === undefined ? [] : [filter.address],
      topic0: filter.topic0 === undefined ? [] : [filter.topic0],
      topic1: filter.topic1 === undefined ? [] : [filter.topic1],
      limit: filter.limit === undefined ? [] : [filter.limit],
    },
    cursor ? [cursor] : [],
    limit
  );
}

export async function getRpcLogsViaGateway(filter: EthJsonRpcLogsFilterInput): Promise<EthLogItemView[]> {
  const cfg = loadConfig(process.env);
  if (!cfg.rpcGatewayUrl) {
    throw new Error("EXPLORER_RPC_GATEWAY_URL is required for logs query");
  }
  const payload: Record<string, unknown> = {};
  if (filter.fromBlock !== undefined) {
    payload.fromBlock = toQuantityHex(filter.fromBlock);
  }
  if (filter.toBlock !== undefined) {
    payload.toBlock = toQuantityHex(filter.toBlock);
  }
  if (filter.address !== undefined) {
    payload.address = normalizeHex(filter.address);
  }
  if (filter.topic0 !== undefined) {
    payload.topics = [normalizeHex(filter.topic0)];
  }
  if (filter.blockHash !== undefined) {
    payload.blockHash = normalizeHex(filter.blockHash);
  }
  const body = {
    jsonrpc: "2.0",
    id: 1,
    method: "eth_getLogs",
    params: [payload],
  };
  const response = await fetch(cfg.rpcGatewayUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    throw new Error(`gateway request failed: HTTP ${response.status}`);
  }
  const json = (await response.json()) as {
    jsonrpc: string;
    id: number | string | null;
    result?: Array<{
      address: string;
      topics: string[];
      data: string;
      blockNumber: string;
      transactionIndex: string;
      logIndex: string;
      transactionHash: string;
    }>;
    error?: { code: number; message: string; data?: unknown };
  };
  if (json.error) {
    const detail = json.error.data === undefined ? "" : ` data=${JSON.stringify(json.error.data)}`;
    throw new Error(`gateway error: ${json.error.code} ${json.error.message}${detail}`);
  }
  const result = json.result ?? [];
  return result.map((item) => ({
    tx_hash: parseHex(item.transactionHash),
    eth_tx_hash: [parseHex(item.transactionHash)],
    block_number: parseQuantityHex(item.blockNumber),
    tx_index: Number(parseQuantityHex(item.transactionIndex)),
    log_index: Number(parseQuantityHex(item.logIndex)),
    address: parseHex(item.address),
    topics: item.topics.map((topic) => parseHex(topic)),
    data: parseDataHex(item.data),
  }));
}

export async function getRpcPending(txId: Uint8Array): Promise<PendingStatusView> {
  return (await getActor()).get_pending(txId);
}

export async function getRpcPruneStatus(): Promise<PruneStatusView> {
  return (await getActor()).get_prune_status();
}

export async function getRpcBalance(address: Uint8Array): Promise<bigint> {
  const out = await (await getActor()).rpc_eth_get_balance(address, { Latest: null });
  if ("Err" in out) {
    throw new Error(out.Err.message);
  }
  return bytesToBigInt(out.Ok);
}

export async function getRpcCode(address: Uint8Array): Promise<Uint8Array> {
  const out = await (await getActor()).rpc_eth_get_code(address, { Latest: null });
  if ("Err" in out) {
    throw new Error(out.Err.message);
  }
  return out.Ok;
}

export async function getRpcExpectedNonce(address: Uint8Array): Promise<bigint> {
  const out = await (await getActor()).expected_nonce_by_address(address);
  if ("Err" in out) {
    throw new Error(out.Err);
  }
  return out.Ok;
}

export async function getRpcCallObject(call: RpcCallObjectView): Promise<ResultRpcCall> {
  return (await getActor()).rpc_eth_call_object(call);
}

function toQuantityHex(value: bigint): string {
  return `0x${value.toString(16)}`;
}

function parseQuantityHex(value: string): bigint {
  const normalized = normalizeHex(value);
  if (!/^0x[0-9a-f]+$/.test(normalized)) {
    throw new Error(`invalid quantity hex: ${value}`);
  }
  return BigInt(normalized);
}

function parseDataHex(value: string): Uint8Array {
  const normalized = normalizeHex(value);
  if (normalized === "0x") {
    return new Uint8Array();
  }
  return parseHex(normalized);
}

async function getActor(): Promise<ExplorerActorMethods> {
  if (cachedActor) {
    return cachedActor;
  }
  const cfg = loadConfig(process.env);
  if (!cfg.canisterId) {
    throw new Error("EVM_CANISTER_ID is required for RPC queries");
  }
  const agent = new HttpAgent({ host: cfg.icHost, fetch: globalThis.fetch });
  if (cfg.fetchRootKey) {
    await agent.fetchRootKey();
  }
  cachedActor = Actor.createActor<ExplorerActorMethods>(idlFactory, {
    agent,
    canisterId: cfg.canisterId,
  });
  return cachedActor;
}

const idlFactory: IDL.InterfaceFactory = ({ IDL }) => {
  const logView = IDL.Record({ data: IDL.Vec(IDL.Nat8), topics: IDL.Vec(IDL.Vec(IDL.Nat8)), address: IDL.Vec(IDL.Nat8) });
  const receiptView = IDL.Record({
    effective_gas_price: IDL.Nat64,
    status: IDL.Nat8,
    l1_data_fee: IDL.Nat,
    tx_id: IDL.Vec(IDL.Nat8),
    tx_index: IDL.Nat32,
    return_data_hash: IDL.Vec(IDL.Nat8),
    logs: IDL.Vec(logView),
    return_data: IDL.Opt(IDL.Vec(IDL.Nat8)),
    total_fee: IDL.Nat,
    block_number: IDL.Nat64,
    operator_fee: IDL.Nat,
    gas_used: IDL.Nat64,
    contract_address: IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const lookupError = IDL.Variant({ NotFound: IDL.Null, Pruned: IDL.Record({ pruned_before_block: IDL.Nat64 }), Pending: IDL.Null });
  const txKindView = IDL.Variant({ EthSigned: IDL.Null, IcSynthetic: IDL.Null });
  const rpcTxView = IDL.Record({
    kind: txKindView,
    hash: IDL.Vec(IDL.Nat8),
    tx_index: IDL.Opt(IDL.Nat32),
    block_number: IDL.Opt(IDL.Nat64),
    eth_tx_hash: IDL.Opt(IDL.Vec(IDL.Nat8)),
    caller_principal: IDL.Opt(IDL.Vec(IDL.Nat8)),
    decode_ok: IDL.Bool,
    decoded: IDL.Opt(IDL.Record({
      from: IDL.Vec(IDL.Nat8),
      to: IDL.Opt(IDL.Vec(IDL.Nat8)),
      value: IDL.Vec(IDL.Nat8),
      input: IDL.Vec(IDL.Nat8),
      nonce: IDL.Nat64,
      gas_limit: IDL.Nat64,
      gas_price: IDL.Opt(IDL.Nat),
      max_fee_per_gas: IDL.Opt(IDL.Nat),
      max_priority_fee_per_gas: IDL.Opt(IDL.Nat),
      chain_id: IDL.Opt(IDL.Nat64),
    })),
    raw: IDL.Vec(IDL.Nat8),
  });
  const ethBlockView = IDL.Record({
    txs: IDL.Variant({ Full: IDL.Vec(rpcTxView), Hashes: IDL.Vec(IDL.Vec(IDL.Nat8)) }),
    block_hash: IDL.Vec(IDL.Nat8),
    number: IDL.Nat64,
    timestamp: IDL.Nat64,
    beneficiary: IDL.Vec(IDL.Nat8),
    state_root: IDL.Vec(IDL.Nat8),
    parent_hash: IDL.Vec(IDL.Nat8),
    gas_limit: IDL.Opt(IDL.Nat64),
    gas_used: IDL.Opt(IDL.Nat64),
    base_fee_per_gas: IDL.Opt(IDL.Nat64),
  });
  const pruneStatusView = IDL.Record({
    pruning_enabled: IDL.Bool,
    prune_running: IDL.Bool,
    estimated_kept_bytes: IDL.Nat64,
    high_water_bytes: IDL.Nat64,
    low_water_bytes: IDL.Nat64,
    hard_emergency_bytes: IDL.Nat64,
    last_prune_at: IDL.Nat64,
    pruned_before_block: IDL.Opt(IDL.Nat64),
    oldest_kept_block: IDL.Opt(IDL.Nat64),
    oldest_kept_timestamp: IDL.Opt(IDL.Nat64),
    need_prune: IDL.Bool,
  });
  const ethLogsCursorView = IDL.Record({ block_number: IDL.Nat64, tx_index: IDL.Nat32, log_index: IDL.Nat32 });
  const ethLogItemView = IDL.Record({
    tx_hash: IDL.Vec(IDL.Nat8),
    eth_tx_hash: IDL.Opt(IDL.Vec(IDL.Nat8)),
    block_number: IDL.Nat64,
    tx_index: IDL.Nat32,
    log_index: IDL.Nat32,
    address: IDL.Vec(IDL.Nat8),
    topics: IDL.Vec(IDL.Vec(IDL.Nat8)),
    data: IDL.Vec(IDL.Nat8),
  });
  const ethLogsPageView = IDL.Record({ items: IDL.Vec(ethLogItemView), next_cursor: IDL.Opt(ethLogsCursorView) });
  const getLogsErrorView = IDL.Variant({
    TooManyResults: IDL.Null,
    RangeTooLarge: IDL.Null,
    InvalidArgument: IDL.Text,
    UnsupportedFilter: IDL.Text,
  });
  const ethLogFilterView = IDL.Record({
    limit: IDL.Opt(IDL.Nat32),
    topic0: IDL.Opt(IDL.Vec(IDL.Nat8)),
    topic1: IDL.Opt(IDL.Vec(IDL.Nat8)),
    address: IDL.Opt(IDL.Vec(IDL.Nat8)),
    to_block: IDL.Opt(IDL.Nat64),
    from_block: IDL.Opt(IDL.Nat64),
  });
  const ethReceiptView = IDL.Record({
    effective_gas_price: IDL.Nat64,
    status: IDL.Nat8,
    l1_data_fee: IDL.Nat,
    tx_index: IDL.Nat32,
    logs: IDL.Vec(IDL.Record({ log_index: IDL.Nat32, data: IDL.Vec(IDL.Nat8), topics: IDL.Vec(IDL.Vec(IDL.Nat8)), address: IDL.Vec(IDL.Nat8) })),
    total_fee: IDL.Nat,
    block_number: IDL.Nat64,
    operator_fee: IDL.Nat,
    eth_tx_hash: IDL.Opt(IDL.Vec(IDL.Nat8)),
    gas_used: IDL.Nat64,
    contract_address: IDL.Opt(IDL.Vec(IDL.Nat8)),
    tx_hash: IDL.Vec(IDL.Nat8),
  });
  const pendingStatusView = IDL.Variant({
    Queued: IDL.Record({ seq: IDL.Nat64 }),
    Included: IDL.Record({ tx_index: IDL.Nat32, block_number: IDL.Nat64 }),
    Unknown: IDL.Null,
    Dropped: IDL.Record({ code: IDL.Nat16 }),
  });
  const rpcReceiptLookupView = IDL.Variant({
    NotFound: IDL.Null,
    Found: ethReceiptView,
    PossiblyPruned: IDL.Record({ pruned_before_block: IDL.Nat64 }),
    Pruned: IDL.Record({ pruned_before_block: IDL.Nat64 }),
  });
  const rpcAccessListItemView = IDL.Record({
    storage_keys: IDL.Vec(IDL.Vec(IDL.Nat8)),
    address: IDL.Vec(IDL.Nat8),
  });
  const rpcCallObjectView = IDL.Record({
    to: IDL.Opt(IDL.Vec(IDL.Nat8)),
    gas: IDL.Opt(IDL.Nat64),
    value: IDL.Opt(IDL.Vec(IDL.Nat8)),
    max_priority_fee_per_gas: IDL.Opt(IDL.Nat),
    data: IDL.Opt(IDL.Vec(IDL.Nat8)),
    from: IDL.Opt(IDL.Vec(IDL.Nat8)),
    max_fee_per_gas: IDL.Opt(IDL.Nat),
    chain_id: IDL.Opt(IDL.Nat64),
    nonce: IDL.Opt(IDL.Nat64),
    tx_type: IDL.Opt(IDL.Nat64),
    access_list: IDL.Opt(IDL.Vec(rpcAccessListItemView)),
    gas_price: IDL.Opt(IDL.Nat),
  });
  const rpcCallResultView = IDL.Record({
    status: IDL.Nat8,
    return_data: IDL.Vec(IDL.Nat8),
    gas_used: IDL.Nat64,
    revert_data: IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const rpcBlockTagView = IDL.Variant({
    Earliest: IDL.Null,
    Safe: IDL.Null,
    Finalized: IDL.Null,
    Latest: IDL.Null,
    Number: IDL.Nat64,
    Pending: IDL.Null,
  });
  const rpcErrorView = IDL.Record({ code: IDL.Nat32, message: IDL.Text });

  return IDL.Service({
    rpc_eth_block_number: IDL.Func([], [IDL.Nat64], ["query"]),
    expected_nonce_by_address: IDL.Func([IDL.Vec(IDL.Nat8)], [IDL.Variant({ Ok: IDL.Nat64, Err: IDL.Text })], ["query"]),
    get_receipt: IDL.Func([IDL.Vec(IDL.Nat8)], [IDL.Variant({ Ok: receiptView, Err: lookupError })], ["query"]),
    get_prune_status: IDL.Func([], [pruneStatusView], ["query"]),
    get_pending: IDL.Func([IDL.Vec(IDL.Nat8)], [pendingStatusView], ["query"]),
    rpc_eth_get_balance: IDL.Func(
      [IDL.Vec(IDL.Nat8), rpcBlockTagView],
      [IDL.Variant({ Ok: IDL.Vec(IDL.Nat8), Err: rpcErrorView })],
      ["query"]
    ),
    rpc_eth_get_block_by_number: IDL.Func([IDL.Nat64, IDL.Bool], [IDL.Opt(ethBlockView)], ["query"]),
    rpc_eth_get_code: IDL.Func(
      [IDL.Vec(IDL.Nat8), rpcBlockTagView],
      [IDL.Variant({ Ok: IDL.Vec(IDL.Nat8), Err: rpcErrorView })],
      ["query"]
    ),
    rpc_eth_get_logs_paged: IDL.Func([ethLogFilterView, IDL.Opt(ethLogsCursorView), IDL.Nat32], [IDL.Variant({ Ok: ethLogsPageView, Err: getLogsErrorView })], ["query"]),
    rpc_eth_call_object: IDL.Func([rpcCallObjectView], [IDL.Variant({ Ok: rpcCallResultView, Err: rpcErrorView })], ["query"]),
    rpc_eth_get_transaction_by_tx_id: IDL.Func([IDL.Vec(IDL.Nat8)], [IDL.Opt(rpcTxView)], ["query"]),
    rpc_eth_get_transaction_by_eth_hash: IDL.Func([IDL.Vec(IDL.Nat8)], [IDL.Opt(rpcTxView)], ["query"]),
    rpc_eth_get_transaction_receipt_with_status_by_eth_hash: IDL.Func([IDL.Vec(IDL.Nat8)], [rpcReceiptLookupView], ["query"]),
    rpc_eth_get_transaction_receipt_with_status_by_tx_id: IDL.Func([IDL.Vec(IDL.Nat8)], [rpcReceiptLookupView], ["query"]),
  });
};
