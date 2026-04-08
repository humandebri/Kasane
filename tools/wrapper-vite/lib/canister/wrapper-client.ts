// どこで: gateway canister クライアント / 何を: estimate / dispatch 参照 / submit_ic_tx を提供する / なぜ: 新 gateway API を UI と script から再利用するため

import type { ActorSubclass, Identity } from "@icp-sdk/core/agent";
import { IDL } from "@icp-sdk/core/candid";
import { loadConfig, type WrapperConfig } from "../config";
import type { DispatchResultView, DispatchStatus } from "../types";
import { callerEvmAddressFromPrincipalText } from "../principal";
import { createActorCache, createIdentityActor, createQueryActor } from "./actor-utils";
import {
  applyWrapGasHeadroom,
  applyUnwrapGasHeadroom,
  buildUnwrapEstimateCallObject,
  buildWrapEstimateCallObject,
  type BuildUnwrapEstimateCallArgs,
  type BuildWrapEstimateCallArgs,
  type WrapEstimateCallObject,
  validateEstimatedGasLimit,
} from "../wrap-estimate";

type ApiErrorWire =
  | { InvalidArgument: { code: string; message: string } }
  | { Rejected: { code: string; message: string } }
  | { Internal: { code: string; message: string } };
type SubmitTxError = { Internal: string } | { Rejected: string } | { InvalidArgument: string };
type SubmitIcTxResult = { Ok: Uint8Array } | { Err: SubmitTxError };
type NatResult = { Ok: bigint } | { Err: string };
type RpcErrorView = { code: number; message: string; error_prefix: [] | [string] };
type GasPriceResult = { Ok: bigint } | { Err: RpcErrorView };
type GasEstimateResult = { Ok: bigint } | { Err: RpcErrorView };
type CallResult = { Ok: { status: number; gas_used: bigint; return_data: Uint8Array; revert_data: [] | [Uint8Array] } } | { Err: RpcErrorView };
type EstimateIcTxResult = {
  Ok: {
    gas_limit: bigint;
    suggested_max_fee_per_gas: bigint;
    suggested_max_priority_fee_per_gas: bigint;
  }
} | { Err: ApiErrorWire };
type UnwrapDispatchOverviewWire = {
  request_id: Uint8Array;
  status: { Queued: null } | { Dispatching: null } | { Dispatched: null } | { DispatchFailed: null };
  error: [] | [string];
};

type WrapperActor = ActorSubclass<{
  expected_nonce_by_address: (address: Uint8Array) => Promise<NatResult>;
  rpc_eth_gas_price: () => Promise<GasPriceResult>;
  rpc_eth_max_priority_fee_per_gas: () => Promise<GasPriceResult>;
  rpc_eth_estimate_gas_object: (call: WrapEstimateCallObject) => Promise<GasEstimateResult>;
  rpc_eth_call_object: (call: WrapEstimateCallObject) => Promise<CallResult>;
  submit_ic_tx: (args: {
    to: [] | [Uint8Array];
    from: [] | [Uint8Array];
    value: bigint;
    max_priority_fee_per_gas: bigint;
    data: Uint8Array;
    max_fee_per_gas: bigint;
    nonce: bigint;
    gas_limit: bigint;
  }) => Promise<SubmitIcTxResult>;
  estimate_ic_tx: (args: {
    to: [] | [Uint8Array];
    from: [] | [Uint8Array];
    value: bigint;
    max_priority_fee_per_gas: bigint;
    data: Uint8Array;
    max_fee_per_gas: bigint;
    nonce: bigint;
    gas_limit: bigint;
  }) => Promise<EstimateIcTxResult>;
  get_unwrap_request_ids_by_tx_id: (txId: Uint8Array) => Promise<Array<Uint8Array>>;
  get_unwrap_dispatch_overview: (requestId: Uint8Array) => Promise<[] | [UnwrapDispatchOverviewWire]>;
}>;

type QueryActorLike = Pick<
  WrapperActor,
  | "expected_nonce_by_address"
  | "rpc_eth_gas_price"
  | "rpc_eth_max_priority_fee_per_gas"
  | "rpc_eth_estimate_gas_object"
  | "rpc_eth_call_object"
  | "estimate_ic_tx"
  | "get_unwrap_request_ids_by_tx_id"
  | "get_unwrap_dispatch_overview"
>;
type SubmitActorLike = Pick<WrapperActor, "submit_ic_tx">;

const actorCache = createActorCache<QueryActorLike, SubmitActorLike, WrapperActor>();
type WrapperClientDeps = {
  loadConfig: () => WrapperConfig;
};

const defaultWrapperClientDeps: WrapperClientDeps = {
  loadConfig,
};

let wrapperClientDeps: WrapperClientDeps = defaultWrapperClientDeps;

const wrapperIdlFactory: IDL.InterfaceFactory = ({ IDL: I }) => {
  const ApiErrorDetail = I.Record({ code: I.Text, message: I.Text });
  const ApiError = I.Variant({
    InvalidArgument: ApiErrorDetail,
    Rejected: ApiErrorDetail,
    Internal: ApiErrorDetail,
  });
  const SubmitTxError = I.Variant({
    Internal: I.Text,
    Rejected: I.Text,
    InvalidArgument: I.Text,
  });
  const SubmitIcTxArgsDto = I.Record({
    to: I.Opt(I.Vec(I.Nat8)),
    from: I.Opt(I.Vec(I.Nat8)),
    value: I.Nat,
    max_priority_fee_per_gas: I.Nat,
    data: I.Vec(I.Nat8),
    max_fee_per_gas: I.Nat,
    nonce: I.Nat64,
    gas_limit: I.Nat64,
  });
  const RpcError = I.Record({
    code: I.Nat32,
    message: I.Text,
    error_prefix: I.Opt(I.Text),
  });
  const RequestDispatchStatus = I.Variant({
    Queued: I.Null,
    Dispatching: I.Null,
    Dispatched: I.Null,
    DispatchFailed: I.Null,
  });
  return I.Service({
    expected_nonce_by_address: I.Func([I.Vec(I.Nat8)], [I.Variant({ Ok: I.Nat64, Err: I.Text })], ["query"]),
    rpc_eth_gas_price: I.Func([], [I.Variant({ Ok: I.Nat, Err: RpcError })], ["query"]),
    rpc_eth_max_priority_fee_per_gas: I.Func([], [I.Variant({ Ok: I.Nat, Err: RpcError })], ["query"]),
    rpc_eth_estimate_gas_object: I.Func([I.Record({
      to: I.Opt(I.Vec(I.Nat8)),
      from: I.Opt(I.Vec(I.Nat8)),
      gas: I.Opt(I.Nat64),
      gas_price: I.Opt(I.Nat),
      nonce: I.Opt(I.Nat64),
      max_fee_per_gas: I.Opt(I.Nat),
      max_priority_fee_per_gas: I.Opt(I.Nat),
      chain_id: I.Opt(I.Nat64),
      tx_type: I.Opt(I.Nat64),
      access_list: I.Opt(I.Vec(I.Record({ address: I.Vec(I.Nat8), storage_keys: I.Vec(I.Vec(I.Nat8)) }))),
      value: I.Opt(I.Vec(I.Nat8)),
      data: I.Opt(I.Vec(I.Nat8)),
    })], [I.Variant({ Ok: I.Nat64, Err: RpcError })], ["query"]),
    rpc_eth_call_object: I.Func([I.Record({
      to: I.Opt(I.Vec(I.Nat8)),
      from: I.Opt(I.Vec(I.Nat8)),
      gas: I.Opt(I.Nat64),
      gas_price: I.Opt(I.Nat),
      nonce: I.Opt(I.Nat64),
      max_fee_per_gas: I.Opt(I.Nat),
      max_priority_fee_per_gas: I.Opt(I.Nat),
      chain_id: I.Opt(I.Nat64),
      tx_type: I.Opt(I.Nat64),
      access_list: I.Opt(I.Vec(I.Record({ address: I.Vec(I.Nat8), storage_keys: I.Vec(I.Vec(I.Nat8)) }))),
      value: I.Opt(I.Vec(I.Nat8)),
      data: I.Opt(I.Vec(I.Nat8)),
    })], [I.Variant({
      Ok: I.Record({
        status: I.Nat8,
        gas_used: I.Nat64,
        return_data: I.Vec(I.Nat8),
        revert_data: I.Opt(I.Vec(I.Nat8)),
      }),
      Err: RpcError,
    })], ["query"]),
    submit_ic_tx: I.Func([SubmitIcTxArgsDto], [I.Variant({ Ok: I.Vec(I.Nat8), Err: SubmitTxError })], []),
    estimate_ic_tx: I.Func([SubmitIcTxArgsDto], [I.Variant({
      Ok: I.Record({
        gas_limit: I.Nat64,
        suggested_max_fee_per_gas: I.Nat,
        suggested_max_priority_fee_per_gas: I.Nat,
      }),
      Err: ApiError,
    })], ["query"]),
    get_unwrap_request_ids_by_tx_id: I.Func([I.Vec(I.Nat8)], [I.Vec(I.Vec(I.Nat8))], ["query"]),
    get_unwrap_dispatch_overview: I.Func(
      [I.Vec(I.Nat8)],
      [I.Opt(I.Record({
        request_id: I.Vec(I.Nat8),
        status: RequestDispatchStatus,
        error: I.Opt(I.Text),
      }))],
      ["query"]
    ),
  });
};

async function getQueryActor(): Promise<QueryActorLike> {
  return actorCache.getQueryActor(async () => {
    const cfg = wrapperClientDeps.loadConfig();
    return createQueryActor<WrapperActor>({
      canisterId: cfg.kasaneEvmCanisterId,
      idlFactory: wrapperIdlFactory,
    });
  });
}

async function getSubmitActor(identity: Identity): Promise<SubmitActorLike> {
  return actorCache.getSubmitActor(identity, async (nextIdentity) => {
    const cfg = wrapperClientDeps.loadConfig();
    return createIdentityActor<WrapperActor>({
      canisterId: cfg.kasaneEvmCanisterId,
      idlFactory: wrapperIdlFactory,
      identity: nextIdentity,
    });
  });
}

function decodeDispatchStatus(status: UnwrapDispatchOverviewWire["status"]): DispatchStatus {
  if ("Queued" in status) {
    return "Queued";
  }
  if ("Dispatching" in status) {
    return "Dispatching";
  }
  if ("Dispatched" in status) {
    return "Dispatched";
  }
  return "DispatchFailed";
}

function decodeSubmitError(err: SubmitTxError): string {
  if ("Internal" in err) {
    return `evm_gateway.submit.internal:${err.Internal}`;
  }
  if ("Rejected" in err) {
    return `evm_gateway.submit.rejected:${err.Rejected}`;
  }
  return `evm_gateway.submit.invalid_argument:${err.InvalidArgument}`;
}

function decodeRpcNatError(prefix: string, err: RpcErrorView): string {
  return `${prefix}:${err.code}:${err.message}`;
}

function decodeApiError(err: ApiErrorWire): string {
  if ("InvalidArgument" in err) {
    return `evm_gateway.invalid_argument:${err.InvalidArgument.code}:${err.InvalidArgument.message}`;
  }
  if ("Rejected" in err) {
    return `evm_gateway.rejected:${err.Rejected.code}:${err.Rejected.message}`;
  }
  return `evm_gateway.internal:${err.Internal.code}:${err.Internal.message}`;
}

export async function getExpectedNonce(callerEvmAddress: Uint8Array): Promise<bigint> {
  const out = await (await getQueryActor()).expected_nonce_by_address(callerEvmAddress);
  if ("Err" in out) {
    throw new Error(`evm_gateway.nonce_failed:${out.Err}`);
  }
  return out.Ok;
}

export async function getWrapEvmNonce(
  wrapCanisterId: string,
  deps: {
    readExpectedNonce: (callerEvmAddress: Uint8Array) => Promise<bigint>;
  } = { readExpectedNonce: getExpectedNonce },
): Promise<bigint> {
  return deps.readExpectedNonce(callerEvmAddressFromPrincipalText(wrapCanisterId.trim()));
}

export async function getGasPriceWei(): Promise<bigint> {
  const out = await (await getQueryActor()).rpc_eth_gas_price();
  if ("Err" in out) {
    throw new Error(decodeRpcNatError("evm_gateway.gas_price_failed", out.Err));
  }
  return out.Ok;
}

export async function getMaxPriorityFeePerGasWei(): Promise<bigint> {
  const out = await (await getQueryActor()).rpc_eth_max_priority_fee_per_gas();
  if ("Err" in out) {
    throw new Error(decodeRpcNatError("evm_gateway.priority_fee_failed", out.Err));
  }
  return out.Ok;
}

export async function estimateWrapGasLimit(
  args: BuildWrapEstimateCallArgs,
  deps: { readEstimateGas: (call: WrapEstimateCallObject) => Promise<GasEstimateResult> } = {
    readEstimateGas: async (call) => (await getQueryActor()).rpc_eth_estimate_gas_object(call),
  },
): Promise<bigint> {
  const out = await deps.readEstimateGas(buildWrapEstimateCallObject(args));
  if ("Err" in out) {
    throw new Error(decodeRpcNatError("evm_gateway.estimate_gas_failed", out.Err));
  }
  return applyWrapGasHeadroom(out.Ok);
}

export async function estimateUnwrapGasLimit(
  args: BuildUnwrapEstimateCallArgs,
  deps: { readEstimateGas: (call: WrapEstimateCallObject) => Promise<GasEstimateResult> } = {
    readEstimateGas: async (call) => (await getQueryActor()).rpc_eth_estimate_gas_object(call),
  },
): Promise<bigint> {
  const out = await deps.readEstimateGas(buildUnwrapEstimateCallObject(args));
  if ("Err" in out) {
    throw new Error(decodeRpcNatError("evm_gateway.estimate_gas_failed", out.Err));
  }
  return applyUnwrapGasHeadroom(out.Ok);
}

export async function estimateContractGasLimit(
  args: { to: Uint8Array; from: Uint8Array; nonce: bigint; data: Uint8Array },
  deps: { readEstimateGas: (call: WrapEstimateCallObject) => Promise<GasEstimateResult> } = {
    readEstimateGas: async (call) => (await getQueryActor()).rpc_eth_estimate_gas_object(call),
  },
): Promise<bigint> {
  const out = await deps.readEstimateGas({
    to: [args.to],
    from: [args.from],
    gas: [],
    gas_price: [],
    nonce: [args.nonce],
    max_fee_per_gas: [],
    max_priority_fee_per_gas: [],
    chain_id: [],
    tx_type: [],
    access_list: [],
    value: [new Uint8Array(32)],
    data: [args.data],
  });
  if ("Err" in out) {
    throw new Error(decodeRpcNatError("evm_gateway.estimate_gas_failed", out.Err));
  }
  return applyWrapGasHeadroom(out.Ok);
}

export async function callReadonlyContract(args: {
  to: Uint8Array;
  data: Uint8Array;
  from?: Uint8Array;
}): Promise<Uint8Array> {
  const out = await (await getQueryActor()).rpc_eth_call_object({
    to: [args.to],
    from: args.from ? [args.from] : [],
    gas: [],
    gas_price: [],
    nonce: [],
    max_fee_per_gas: [],
    max_priority_fee_per_gas: [],
    chain_id: [],
    tx_type: [],
    access_list: [],
    value: [new Uint8Array(32)],
    data: [args.data],
  });
  if ("Err" in out) {
    throw new Error(decodeRpcNatError("evm_gateway.call_failed", out.Err));
  }
  if (out.Ok.status !== 1) {
    throw new Error("evm_gateway.call_reverted");
  }
  return out.Ok.return_data;
}

export async function estimateIcTx(args: {
  from: Uint8Array;
  to: Uint8Array;
  data: Uint8Array;
  nonce: bigint;
  gasLimit: bigint;
}): Promise<{
  gasLimit: bigint;
  suggestedMaxFeePerGas: bigint;
  suggestedMaxPriorityFeePerGas: bigint;
}> {
  const out = await (await getQueryActor()).estimate_ic_tx({
    to: [args.to],
    from: [args.from],
    value: 0n,
    max_priority_fee_per_gas: 0n,
    data: args.data,
    max_fee_per_gas: 0n,
    nonce: args.nonce,
    gas_limit: args.gasLimit,
  });
  if ("Err" in out) {
    throw new Error(decodeApiError(out.Err));
  }
  return {
    gasLimit: out.Ok.gas_limit,
    suggestedMaxFeePerGas: out.Ok.suggested_max_fee_per_gas,
    suggestedMaxPriorityFeePerGas: out.Ok.suggested_max_priority_fee_per_gas,
  };
}

export async function submitIcTx(args: {
  to: Uint8Array;
  data: Uint8Array;
  nonce: bigint;
  gasLimit: bigint;
  identity: Identity;
  maxFeePerGas?: bigint;
  maxPriorityFeePerGas?: bigint;
}): Promise<Uint8Array> {
  const [maxFeePerGas, maxPriorityFeePerGas] = await Promise.all([
    args.maxFeePerGas === undefined ? getGasPriceWei() : Promise.resolve(args.maxFeePerGas),
    args.maxPriorityFeePerGas === undefined
      ? getMaxPriorityFeePerGasWei()
      : Promise.resolve(args.maxPriorityFeePerGas),
  ]);
  const out = await (await getSubmitActor(args.identity)).submit_ic_tx({
    to: [args.to],
    from: [],
    value: 0n,
    max_priority_fee_per_gas: maxPriorityFeePerGas,
    data: args.data,
    max_fee_per_gas: maxFeePerGas,
    nonce: args.nonce,
    gas_limit: args.gasLimit,
  });
  if ("Err" in out) {
    throw new Error(decodeSubmitError(out.Err));
  }
  return out.Ok;
}

export async function getDispatchResult(requestId: Uint8Array): Promise<DispatchResultView | null> {
  const out = await (await getQueryActor()).get_unwrap_dispatch_overview(requestId);
  if (out.length === 0) {
    return null;
  }
  return {
    status: decodeDispatchStatus(out[0].status),
    errorCode: out[0].error[0] ?? null,
  };
}

export async function getUnwrapRequestIdsByTxId(txId: Uint8Array): Promise<Uint8Array[]> {
  return (await getQueryActor()).get_unwrap_request_ids_by_tx_id(txId);
}

export const wrapperClientTestHooks = {
  reset(): void {
    actorCache.reset();
    wrapperClientDeps = defaultWrapperClientDeps;
  },
  setMockQueryActor(actor: QueryActorLike | null): void {
    actorCache.setMockQueryActor(actor);
  },
  setMockSubmitActor(actor: SubmitActorLike | null): void {
    actorCache.setMockSubmitActor(actor);
  },
  setDeps(deps: Partial<WrapperClientDeps>): void {
    wrapperClientDeps = {
      ...defaultWrapperClientDeps,
      ...deps,
    };
  },
  decodeSubmitError,
  decodeRpcNatError,
};
