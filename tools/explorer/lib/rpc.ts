// どこで: Explorer RPC層 / 何を: canister query を型付きで実行 / なぜ: receipt/head情報をSQLite外から取得するため

import { Actor, HttpAgent } from "@dfinity/agent";
import type { IDL } from "@dfinity/candid";
import { loadConfig } from "./config";

export type LookupError = { NotFound: null } | { Pending: null } | { Pruned: { pruned_before_block: bigint } };

export type LogView = {
  address: Uint8Array;
  topics: Uint8Array[];
  data: Uint8Array;
};

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

type Result<T, E> = { Ok: T } | { Err: E };

type EthTxView = {
  eth_tx_hash: [] | [Uint8Array];
};

type EthBlockView = {
  block_hash: Uint8Array;
  parent_hash: Uint8Array;
  state_root: Uint8Array;
  number: bigint;
  timestamp: bigint;
  txs: { Full: EthTxView[] } | { Hashes: Uint8Array[] };
};

type ExplorerActorMethods = {
  rpc_eth_block_number: () => Promise<bigint>;
  get_receipt: (txId: Uint8Array) => Promise<Result<ReceiptView, LookupError>>;
  rpc_eth_get_block_by_number: (number: bigint, fullTx: boolean) => Promise<[] | [EthBlockView]>;
};

let cachedActor: ExplorerActorMethods | null = null;

export async function getRpcHeadNumber(): Promise<bigint> {
  const actor = await getActor();
  return actor.rpc_eth_block_number();
}

export async function getReceiptByTxId(txId: Uint8Array): Promise<Result<ReceiptView, LookupError>> {
  const actor = await getActor();
  return actor.get_receipt(txId);
}

export async function getRpcBlock(number: bigint): Promise<EthBlockView | null> {
  return getRpcBlockWithTxMode(number, false);
}

export async function getRpcBlockWithTxMode(number: bigint, fullTx: boolean): Promise<EthBlockView | null> {
  const actor = await getActor();
  const out = await actor.rpc_eth_get_block_by_number(number, fullTx);
  return out.length === 0 ? null : out[0];
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
  const LogView = IDL.Record({
    data: IDL.Vec(IDL.Nat8),
    topics: IDL.Vec(IDL.Vec(IDL.Nat8)),
    address: IDL.Vec(IDL.Nat8),
  });

  const ReceiptView = IDL.Record({
    effective_gas_price: IDL.Nat64,
    status: IDL.Nat8,
    l1_data_fee: IDL.Nat,
    tx_id: IDL.Vec(IDL.Nat8),
    tx_index: IDL.Nat32,
    return_data_hash: IDL.Vec(IDL.Nat8),
    logs: IDL.Vec(LogView),
    return_data: IDL.Opt(IDL.Vec(IDL.Nat8)),
    total_fee: IDL.Nat,
    block_number: IDL.Nat64,
    operator_fee: IDL.Nat,
    gas_used: IDL.Nat64,
    contract_address: IDL.Opt(IDL.Vec(IDL.Nat8)),
  });

  const LookupError = IDL.Variant({
    NotFound: IDL.Null,
    Pruned: IDL.Record({ pruned_before_block: IDL.Nat64 }),
    Pending: IDL.Null,
  });

  const EthTxView = IDL.Record({
    eth_tx_hash: IDL.Opt(IDL.Vec(IDL.Nat8)),
  });

  const EthBlockView = IDL.Record({
    txs: IDL.Variant({ Full: IDL.Vec(EthTxView), Hashes: IDL.Vec(IDL.Vec(IDL.Nat8)) }),
    block_hash: IDL.Vec(IDL.Nat8),
    number: IDL.Nat64,
    timestamp: IDL.Nat64,
    state_root: IDL.Vec(IDL.Nat8),
    parent_hash: IDL.Vec(IDL.Nat8),
  });

  return IDL.Service({
    rpc_eth_block_number: IDL.Func([], [IDL.Nat64], ["query"]),
    get_receipt: IDL.Func(
      [IDL.Vec(IDL.Nat8)],
      [IDL.Variant({ Ok: ReceiptView, Err: LookupError })],
      ["query"]
    ),
    rpc_eth_get_block_by_number: IDL.Func([IDL.Nat64, IDL.Bool], [IDL.Opt(EthBlockView)], ["query"]),
  });
};
