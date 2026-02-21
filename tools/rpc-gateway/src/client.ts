// どこで: Gateway canisterクライアント / 何を: query/updateの呼び出しラッパを提供 / なぜ: ハンドラ側の責務をJSON-RPC変換に集中させるため

import { Actor, HttpAgent } from "@dfinity/agent";
import { readFileSync } from "node:fs";
import { CONFIG } from "./config";
import { idlFactory } from "./candid";
import { identityFromPem } from "./identity";

export type DecodedTxView = {
  to: [] | [Uint8Array];
  value: Uint8Array;
  from: Uint8Array;
  chain_id: [] | [bigint];
  nonce: bigint;
  gas_limit: bigint;
  input: Uint8Array;
  gas_price: [] | [bigint];
  max_fee_per_gas: [] | [bigint];
  max_priority_fee_per_gas: [] | [bigint];
};

export type EthTxView = {
  raw: Uint8Array;
  tx_index: [] | [number];
  decode_ok: boolean;
  hash: Uint8Array;
  kind: { EthSigned: null } | { IcSynthetic: null };
  block_number: [] | [bigint];
  eth_tx_hash: [] | [Uint8Array];
  decoded: [] | [DecodedTxView];
};

export type EthReceiptView = {
  effective_gas_price: bigint;
  status: number;
  l1_data_fee: bigint;
  tx_index: number;
  logs: Array<{ data: Uint8Array; topics: Uint8Array[]; address: Uint8Array; log_index: number }>;
  total_fee: bigint;
  block_number: bigint;
  operator_fee: bigint;
  eth_tx_hash: [] | [Uint8Array];
  gas_used: bigint;
  contract_address: [] | [Uint8Array];
  tx_hash: Uint8Array;
};

export type EthBlockView = {
  txs: { Full: EthTxView[] } | { Hashes: Uint8Array[] };
  block_hash: Uint8Array;
  number: bigint;
  timestamp: bigint;
  beneficiary: Uint8Array;
  state_root: Uint8Array;
  parent_hash: Uint8Array;
  base_fee_per_gas: [] | [bigint];
  gas_limit: [] | [bigint];
  gas_used: [] | [bigint];
};
export type RpcBlockLookupView =
  | { NotFound: null }
  | { Found: EthBlockView }
  | { Pruned: { pruned_before_block: bigint } };
export type EthLogsCursorView = { tx_index: number; log_index: number; block_number: bigint };
export type EthLogItemView = {
  tx_index: number;
  log_index: number;
  data: Uint8Array;
  block_number: bigint;
  topics: Uint8Array[];
  address: Uint8Array;
  eth_tx_hash: [] | [Uint8Array];
  tx_hash: Uint8Array;
};
export type EthLogsPageView = {
  next_cursor: [] | [EthLogsCursorView];
  items: EthLogItemView[];
};
export type EthLogFilterView = {
  limit: [] | [number];
  topic0: [] | [Uint8Array];
  topic1: [] | [Uint8Array];
  address: [] | [Uint8Array];
  to_block: [] | [bigint];
  from_block: [] | [bigint];
};
type GetLogsErrorView =
  | { TooManyResults: null }
  | { RangeTooLarge: null }
  | { InvalidArgument: string }
  | { UnsupportedFilter: string };
export type RpcReceiptLookupView =
  | { NotFound: null }
  | { Found: EthReceiptView }
  | { PossiblyPruned: { pruned_before_block: bigint } }
  | { Pruned: { pruned_before_block: bigint } };

type TextResult = { Ok: Uint8Array } | { Err: string };
type NonceResult = { Ok: bigint } | { Err: string };
export type RpcErrorView = { code: number; message: string };
type Nat64Result = { Ok: bigint } | { Err: RpcErrorView };
type LogsPageResult = { Ok: EthLogsPageView } | { Err: GetLogsErrorView };
type SendErr = { Internal: string } | { Rejected: string } | { InvalidArgument: string };
type SendResult = { Ok: Uint8Array } | { Err: SendErr };
export type CallObject = {
  to: [] | [Uint8Array];
  from: [] | [Uint8Array];
  gas: [] | [bigint];
  gas_price: [] | [bigint];
  nonce: [] | [bigint];
  max_fee_per_gas: [] | [bigint];
  max_priority_fee_per_gas: [] | [bigint];
  chain_id: [] | [bigint];
  tx_type: [] | [bigint];
  access_list: [] | [Array<{ address: Uint8Array; storage_keys: Uint8Array[] }>];
  value: [] | [Uint8Array];
  data: [] | [Uint8Array];
};
export type CallObjectResult = {
  status: number;
  gas_used: bigint;
  return_data: Uint8Array;
  revert_data: [] | [Uint8Array];
};
export type OpsStatusView = {
  needs_migration: boolean;
  critical_corrupt: boolean;
  schema_version: number;
};
type CallResult = { Ok: CallObjectResult } | { Err: RpcErrorView };

type Methods = {
  expected_nonce_by_address: (address: Uint8Array) => Promise<NonceResult>;
  rpc_eth_chain_id: () => Promise<bigint>;
  rpc_eth_block_number: () => Promise<bigint>;
  rpc_eth_get_block_by_number: (number: bigint, fullTx: boolean) => Promise<[] | [EthBlockView]>;
  rpc_eth_get_block_by_number_with_status: (number: bigint, fullTx: boolean) => Promise<RpcBlockLookupView>;
  rpc_eth_get_transaction_by_eth_hash: (ethTxHash: Uint8Array) => Promise<[] | [EthTxView]>;
  rpc_eth_get_transaction_by_tx_id: (txId: Uint8Array) => Promise<[] | [EthTxView]>;
  rpc_eth_get_transaction_receipt_by_eth_hash: (ethTxHash: Uint8Array) => Promise<[] | [EthReceiptView]>;
  rpc_eth_get_transaction_receipt_with_status: (ethTxHash: Uint8Array) => Promise<RpcReceiptLookupView>;
  rpc_eth_get_logs_paged: (
    filter: EthLogFilterView,
    cursor: [] | [EthLogsCursorView],
    limit: number
  ) => Promise<LogsPageResult>;
  rpc_eth_get_balance: (address: Uint8Array) => Promise<TextResult>;
  rpc_eth_get_code: (address: Uint8Array) => Promise<TextResult>;
  rpc_eth_get_storage_at: (address: Uint8Array, slot: Uint8Array) => Promise<TextResult>;
  rpc_eth_call_object: (call: CallObject) => Promise<CallResult>;
  rpc_eth_estimate_gas_object: (call: CallObject) => Promise<Nat64Result>;
  rpc_eth_call_rawtx: (rawTx: Uint8Array) => Promise<TextResult>;
  rpc_eth_send_raw_transaction: (rawTx: Uint8Array) => Promise<SendResult>;
  get_ops_status: () => Promise<OpsStatusView>;
};

let actorPromise: Promise<Methods> | null = null;

export async function getActor(): Promise<Methods> {
  if (!actorPromise) {
    actorPromise = createActor();
  }
  return actorPromise;
}

async function createActor(): Promise<Methods> {
  const fetchFn = globalThis.fetch;
  if (typeof fetchFn !== "function") {
    throw new Error("global fetch is not available; use Node 18+");
  }
  const identity = loadOptionalIdentityFromPem();
  const agent = new HttpAgent({ host: CONFIG.icHost, fetch: fetchFn, identity });
  if (CONFIG.fetchRootKey) {
    await agent.fetchRootKey();
  }
  return Actor.createActor<Methods>(idlFactory, {
    agent,
    canisterId: CONFIG.canisterId,
  });
}

function loadOptionalIdentityFromPem() {
  if (!CONFIG.identityPemPath) {
    return undefined;
  }
  const pem = readFileSync(CONFIG.identityPemPath, "utf8");
  return identityFromPem(pem);
}
