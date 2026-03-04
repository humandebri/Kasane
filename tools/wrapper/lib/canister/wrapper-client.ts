// どこで: wrapper canister クライアント / 何を: dispatch状態とsubmit_ic_txを呼び出す / なぜ: BFF APIでgateway責務を明確に扱うため

import { Actor, type ActorSubclass } from "@dfinity/agent";
import { IDL } from "@dfinity/candid";
import { loadConfig } from "../config";
import type { DispatchResultView, DispatchStatus } from "../types";
import { bytesToHex } from "../utils";
import { getQueryAgent, getSubmitAgent } from "./agent";

type SubmitTxError = { Internal: string } | { Rejected: string } | { InvalidArgument: string };
type SubmitIcTxResult = { Ok: Uint8Array } | { Err: SubmitTxError };
type NatResult = { Ok: bigint } | { Err: string };
type RequestKind = { Unwrap: null };
const REQUEST_KIND_UNWRAP: RequestKind = { Unwrap: null };

type WrapperActor = ActorSubclass<{
  expected_nonce_by_address: (address: Uint8Array) => Promise<NatResult>;
  submit_ic_tx: (args: {
    to: [] | [Uint8Array];
    value: bigint;
    max_priority_fee_per_gas: bigint;
    data: Uint8Array;
    max_fee_per_gas: bigint;
    nonce: bigint;
    gas_limit: bigint;
  }) => Promise<SubmitIcTxResult>;
  get_request_dispatch_status: (kind: RequestKind, requestId: Uint8Array) => Promise<[] | [{ Queued: null } | { Dispatching: null } | { Dispatched: null } | { DispatchFailed: null }]>
  get_request_dispatch_result: (kind: RequestKind, requestId: Uint8Array) => Promise<[] | [{
    status: { Queued: null } | { Dispatching: null } | { Dispatched: null } | { DispatchFailed: null };
    vault_canister_id: Uint8Array;
    error_code: [] | [string];
  }]>;
}>;

let cachedQueryActor: WrapperActor | null = null;
let cachedSubmitActor: WrapperActor | null = null;

const wrapperIdlFactory: IDL.InterfaceFactory = ({ IDL: I }) => {
  const SubmitTxError = I.Variant({
    Internal: I.Text,
    Rejected: I.Text,
    InvalidArgument: I.Text,
  });
  const SubmitIcTxArgsDto = I.Record({
    to: I.Opt(I.Vec(I.Nat8)),
    value: I.Nat,
    max_priority_fee_per_gas: I.Nat,
    data: I.Vec(I.Nat8),
    max_fee_per_gas: I.Nat,
    nonce: I.Nat64,
    gas_limit: I.Nat64,
  });
  const RequestDispatchStatusView = I.Variant({
    Queued: I.Null,
    Dispatching: I.Null,
    Dispatched: I.Null,
    DispatchFailed: I.Null,
  });
  const RequestDispatchResultView = I.Record({
    status: RequestDispatchStatusView,
    vault_canister_id: I.Vec(I.Nat8),
    error_code: I.Opt(I.Text),
  });
  const RequestKindView = I.Variant({
    Unwrap: I.Null,
  });
  return I.Service({
    expected_nonce_by_address: I.Func([I.Vec(I.Nat8)], [I.Variant({ Ok: I.Nat, Err: I.Text })], ["query"]),
    submit_ic_tx: I.Func([SubmitIcTxArgsDto], [I.Variant({ Ok: I.Vec(I.Nat8), Err: SubmitTxError })], []),
    get_request_dispatch_status: I.Func([RequestKindView, I.Vec(I.Nat8)], [I.Opt(RequestDispatchStatusView)], ["query"]),
    get_request_dispatch_result: I.Func([RequestKindView, I.Vec(I.Nat8)], [I.Opt(RequestDispatchResultView)], ["query"]),
  });
};

async function getQueryActor(): Promise<WrapperActor> {
  if (cachedQueryActor) {
    return cachedQueryActor;
  }
  const cfg = loadConfig();
  cachedQueryActor = Actor.createActor<WrapperActor>(wrapperIdlFactory, {
    canisterId: cfg.evmGatewayCanisterId,
    agent: await getQueryAgent(),
  });
  return cachedQueryActor;
}

async function getSubmitActor(): Promise<WrapperActor> {
  if (cachedSubmitActor) {
    return cachedSubmitActor;
  }
  const cfg = loadConfig();
  cachedSubmitActor = Actor.createActor<WrapperActor>(wrapperIdlFactory, {
    canisterId: cfg.evmGatewayCanisterId,
    agent: await getSubmitAgent(),
  });
  return cachedSubmitActor;
}

function decodeDispatchStatus(status: { Queued: null } | { Dispatching: null } | { Dispatched: null } | { DispatchFailed: null }): DispatchStatus {
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

export async function getExpectedNonce(callerEvmAddress: Uint8Array): Promise<bigint> {
  const out = await (await getQueryActor()).expected_nonce_by_address(callerEvmAddress);
  if ("Err" in out) {
    throw new Error(`evm_gateway.nonce_failed:${out.Err}`);
  }
  return out.Ok;
}

export async function submitIcTx(args: {
  to: Uint8Array;
  data: Uint8Array;
  nonce: bigint;
}): Promise<Uint8Array> {
  const out = await (await getSubmitActor()).submit_ic_tx({
    to: [args.to],
    value: 0n,
    max_priority_fee_per_gas: 0n,
    data: args.data,
    max_fee_per_gas: 0n,
    nonce: args.nonce,
    gas_limit: 300_000n,
  });
  if ("Err" in out) {
    throw new Error(decodeSubmitError(out.Err));
  }
  return out.Ok;
}

export async function getDispatchStatus(requestId: Uint8Array): Promise<DispatchStatus | null> {
  const out = await (await getQueryActor()).get_request_dispatch_status(REQUEST_KIND_UNWRAP, requestId);
  if (out.length === 0) {
    return null;
  }
  return decodeDispatchStatus(out[0]);
}

export async function getDispatchResult(requestId: Uint8Array): Promise<DispatchResultView | null> {
  const out = await (await getQueryActor()).get_request_dispatch_result(REQUEST_KIND_UNWRAP, requestId);
  if (out.length === 0) {
    return null;
  }
  const value = out[0];
  return {
    status: decodeDispatchStatus(value.status),
    vaultCanisterId: value.vault_canister_id,
    errorCode: value.error_code.length === 0 ? null : value.error_code[0],
  };
}

export const wrapperClientTestHooks = {
  reset(): void {
    cachedQueryActor = null;
    cachedSubmitActor = null;
  },
  formatTxId: bytesToHex,
};
