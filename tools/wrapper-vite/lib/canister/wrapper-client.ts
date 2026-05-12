// どこで: gateway canister クライアント / 何を: estimate / dispatch 参照 / submit_ic_tx を提供する / なぜ: 新 gateway API を UI と script から再利用するため

import type { ActorSubclass, Identity } from "@icp-sdk/core/agent";
import { idlFactory as wrapperIdlFactory } from "@/src/declarations/evm_canister/evm_canister.did.js";
import type {
  _SERVICE as WrapperService,
  ApiError as ApiErrorWire,
  RpcErrorView,
  SubmitTxError,
  UnwrapDispatchOverviewView,
} from "@/src/declarations/evm_canister/evm_canister.did";
import { loadConfig, type WrapperConfig } from "../config";
import type { DispatchResultView, DispatchStatus } from "../types";
import { callerEvmAddressFromPrincipalText } from "../principal";
import { createActorCache, createAuthenticatedActor, createQueryActor } from "./actor-utils";
import type { AuthenticatedCaller } from "./authenticated-caller";
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

type WrapperActor = ActorSubclass<WrapperService>;
type PlainActorMethod<T> = T extends (...args: infer TArgs) => infer TResult ? (...args: TArgs) => TResult : never;
type NatResult = Awaited<ReturnType<WrapperActor["expected_nonce_by_address"]>>;
type GasPriceResult = Awaited<ReturnType<WrapperActor["rpc_eth_gas_price"]>>;
type GasEstimateResult = Awaited<ReturnType<WrapperActor["rpc_eth_estimate_gas_object"]>>;
type CallResult = Awaited<ReturnType<WrapperActor["rpc_eth_call_object"]>>;
type EstimateIcTxResult = Awaited<ReturnType<WrapperActor["estimate_ic_tx"]>>;

type QueryActorLike = {
  expected_nonce_by_address: PlainActorMethod<WrapperService["expected_nonce_by_address"]>;
  rpc_eth_gas_price: PlainActorMethod<WrapperService["rpc_eth_gas_price"]>;
  rpc_eth_max_priority_fee_per_gas: PlainActorMethod<WrapperService["rpc_eth_max_priority_fee_per_gas"]>;
  rpc_eth_estimate_gas_object: PlainActorMethod<WrapperService["rpc_eth_estimate_gas_object"]>;
  rpc_eth_call_object: PlainActorMethod<WrapperService["rpc_eth_call_object"]>;
  estimate_ic_tx: PlainActorMethod<WrapperService["estimate_ic_tx"]>;
  get_unwrap_request_ids_by_eth_tx_hash: PlainActorMethod<WrapperService["get_unwrap_request_ids_by_eth_tx_hash"]>;
  get_unwrap_request_ids_by_tx_id: PlainActorMethod<WrapperService["get_unwrap_request_ids_by_tx_id"]>;
  get_unwrap_dispatch_overview: PlainActorMethod<WrapperService["get_unwrap_dispatch_overview"]>;
};
type SubmitActorLike = {
  submit_ic_tx: PlainActorMethod<WrapperService["submit_ic_tx"]>;
};

const actorCache = createActorCache<QueryActorLike, SubmitActorLike, WrapperActor>();
type WrapperClientDeps = {
  loadConfig: () => WrapperConfig;
};

const defaultWrapperClientDeps: WrapperClientDeps = {
  loadConfig,
};

let wrapperClientDeps: WrapperClientDeps = defaultWrapperClientDeps;

async function getQueryActor(): Promise<QueryActorLike> {
  return actorCache.getQueryActor(async () => {
    const cfg = wrapperClientDeps.loadConfig();
    return createQueryActor<WrapperActor>({
      canisterId: cfg.kasaneEvmCanisterId,
      idlFactory: wrapperIdlFactory,
    });
  });
}

async function getSubmitActor(caller: AuthenticatedCaller | Identity): Promise<SubmitActorLike> {
  return actorCache.getSubmitActor(caller, async (nextCaller) => {
    const cfg = wrapperClientDeps.loadConfig();
    return createAuthenticatedActor<WrapperActor>({
      canisterId: cfg.kasaneEvmCanisterId,
      idlFactory: wrapperIdlFactory,
      caller: nextCaller,
    });
  });
}

function decodeDispatchStatus(status: UnwrapDispatchOverviewView["status"]): DispatchStatus {
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
  caller: AuthenticatedCaller | Identity;
  maxFeePerGas?: bigint;
  maxPriorityFeePerGas?: bigint;
}): Promise<Uint8Array> {
  const [maxFeePerGas, maxPriorityFeePerGas] = await Promise.all([
    args.maxFeePerGas === undefined ? getGasPriceWei() : Promise.resolve(args.maxFeePerGas),
    args.maxPriorityFeePerGas === undefined
      ? getMaxPriorityFeePerGasWei()
      : Promise.resolve(args.maxPriorityFeePerGas),
  ]);
  const out = await (await getSubmitActor(args.caller)).submit_ic_tx({
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

export async function getUnwrapRequestIdsByEthTxHash(ethTxHash: Uint8Array): Promise<Uint8Array[]> {
  return (await getQueryActor()).get_unwrap_request_ids_by_eth_tx_hash(ethTxHash);
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
