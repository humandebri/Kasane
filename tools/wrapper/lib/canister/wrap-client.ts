// どこで: wrap-canister クライアント / 何を: execution状態照会とwithdraw updateを提供 / なぜ: dispatch結果と実行結果を分離追跡するため

import { Actor, type ActorSubclass, type Identity } from "@dfinity/agent";
import { IDL } from "@dfinity/candid";
import { Principal } from "@dfinity/principal";
import { loadConfig } from "../config";
import type { ExecutionStatus, WrapExecutionResult } from "../types";
import { getIdentityAgent, getQueryAgent } from "./agent";

type RequestStatusVariant = { Queued: null } | { Running: null } | { Succeeded: null } | { Failed: null };
type UnwrapRequestResultWire = {
  status: RequestStatusVariant;
  ledger_tx_id: [] | [Uint8Array];
  error_code: [] | [string];
};
type WrapRequestResultWire = {
  status: RequestStatusVariant;
  pull_ledger_tx_id: [] | [Uint8Array];
  mint_tx_id: [] | [Uint8Array];
  error_code: [] | [string];
  withdrawn: boolean;
  withdraw_ledger_tx_id: [] | [Uint8Array];
  withdraw_error_code: [] | [string];
  mint_failed_recoverable: boolean;
  fee_ledger_tx_id: [] | [Uint8Array];
  charged_fee_e8s: [] | [bigint];
  charged_gas_price_wei: [] | [bigint];
};
type FeePolicyViewWire = {
  fee_ledger_canister: Principal;
  cycle_fee_e8s: bigint;
  gas_price_buffer_bps: number;
};

type WrapActor = ActorSubclass<{
  get_request_result: (requestId: Uint8Array) => Promise<[] | [UnwrapRequestResultWire]>;
  get_wrap_request_result: (requestId: Uint8Array) => Promise<[] | [WrapRequestResultWire]>;
  retry_failed_unwrap: (args: { request_id: Uint8Array }) => Promise<
    | { Ok: { request_id: Uint8Array } }
    | { Err: string }
  >;
  withdraw_failed_wrap: (args: { request_id: Uint8Array }) => Promise<
    | { Ok: { request_id: Uint8Array; ledger_tx_id: Uint8Array } }
    | { Err: string }
  >;
  submit_wrap_request: (args: {
    request_id: Uint8Array;
    asset_id: Uint8Array;
    amount: Uint8Array;
    evm_recipient: Uint8Array;
    evm_nonce: bigint;
    gas_limit: bigint;
  }) => Promise<
    | { Ok: { request_id: Uint8Array } }
    | { Err: string }
  >;
  get_fee_policy: () => Promise<{ Ok: FeePolicyViewWire } | { Err: string }>;
}>;

type ExecutionResultDeps = {
  readUnwrapResult: (requestId: Uint8Array) => Promise<[] | [UnwrapRequestResultWire]>;
  readWrapResult: (requestId: Uint8Array) => Promise<[] | [WrapRequestResultWire]>;
};

let cachedQueryActor: WrapActor | null = null;
const cachedSubmitActors = new Map<string, WrapActor>();

const wrapIdlFactory: IDL.InterfaceFactory = ({ IDL: I }) => {
  const RequestStatus = I.Variant({
    Queued: I.Null,
    Running: I.Null,
    Succeeded: I.Null,
    Failed: I.Null,
  });
  const RequestResult = I.Record({
    status: RequestStatus,
    ledger_tx_id: I.Opt(I.Vec(I.Nat8)),
    error_code: I.Opt(I.Text),
  });
  const WrapRequestResult = I.Record({
    status: RequestStatus,
    pull_ledger_tx_id: I.Opt(I.Vec(I.Nat8)),
    mint_tx_id: I.Opt(I.Vec(I.Nat8)),
    error_code: I.Opt(I.Text),
    withdrawn: I.Bool,
    withdraw_ledger_tx_id: I.Opt(I.Vec(I.Nat8)),
    withdraw_error_code: I.Opt(I.Text),
    mint_failed_recoverable: I.Bool,
    fee_ledger_tx_id: I.Opt(I.Vec(I.Nat8)),
    charged_fee_e8s: I.Opt(I.Nat),
    charged_gas_price_wei: I.Opt(I.Nat),
  });
  const FeePolicyView = I.Record({
    fee_ledger_canister: I.Principal,
    cycle_fee_e8s: I.Nat64,
    gas_price_buffer_bps: I.Nat32,
  });
  const WithdrawFailedWrapArgs = I.Record({
    request_id: I.Vec(I.Nat8),
  });
  const RetryFailedUnwrapArgs = I.Record({
    request_id: I.Vec(I.Nat8),
  });
  const WithdrawFailedWrapOk = I.Record({
    request_id: I.Vec(I.Nat8),
    ledger_tx_id: I.Vec(I.Nat8),
  });
  return I.Service({
    get_request_result: I.Func([I.Vec(I.Nat8)], [I.Opt(RequestResult)], ["query"]),
    get_wrap_request_result: I.Func([I.Vec(I.Nat8)], [I.Opt(WrapRequestResult)], ["query"]),
    submit_wrap_request: I.Func(
      [I.Record({
        request_id: I.Vec(I.Nat8),
        asset_id: I.Vec(I.Nat8),
        amount: I.Vec(I.Nat8),
        evm_recipient: I.Vec(I.Nat8),
        evm_nonce: I.Nat64,
        gas_limit: I.Nat64,
      })],
      [I.Variant({ Ok: I.Record({ request_id: I.Vec(I.Nat8) }), Err: I.Text })],
      []
    ),
    get_fee_policy: I.Func([], [I.Variant({ Ok: FeePolicyView, Err: I.Text })], ["query"]),
    retry_failed_unwrap: I.Func(
      [RetryFailedUnwrapArgs],
      [I.Variant({ Ok: I.Record({ request_id: I.Vec(I.Nat8) }), Err: I.Text })],
      []
    ),
    withdraw_failed_wrap: I.Func(
      [WithdrawFailedWrapArgs],
      [I.Variant({ Ok: WithdrawFailedWrapOk, Err: I.Text })],
      []
    ),
  });
};

async function getQueryActor(): Promise<WrapActor> {
  if (cachedQueryActor) {
    return cachedQueryActor;
  }
  const cfg = loadConfig();
  cachedQueryActor = Actor.createActor<WrapActor>(wrapIdlFactory, {
    canisterId: cfg.wrapCanisterId,
    agent: await getQueryAgent(),
  });
  return cachedQueryActor;
}

async function getSubmitActor(identity: Identity): Promise<WrapActor> {
  const key = identity.getPrincipal().toText();
  const cached = cachedSubmitActors.get(key);
  if (cached) {
    return cached;
  }
  const cfg = loadConfig();
  const actor = Actor.createActor<WrapActor>(wrapIdlFactory, {
    canisterId: cfg.wrapCanisterId,
    agent: await getIdentityAgent(identity),
  });
  cachedSubmitActors.set(key, actor);
  return actor;
}

function decodeExecutionStatus(status: { Queued: null } | { Running: null } | { Succeeded: null } | { Failed: null }): ExecutionStatus {
  if ("Queued" in status) {
    return "Queued";
  }
  if ("Running" in status) {
    return "Running";
  }
  if ("Succeeded" in status) {
    return "Succeeded";
  }
  return "Failed";
}

const defaultExecutionResultDeps: ExecutionResultDeps = {
  readUnwrapResult: async (requestId: Uint8Array) => (await getQueryActor()).get_request_result(requestId),
  readWrapResult: async (requestId: Uint8Array) => (await getQueryActor()).get_wrap_request_result(requestId),
};

export async function getExecutionResult(
  requestId: Uint8Array,
  deps: ExecutionResultDeps = defaultExecutionResultDeps,
): Promise<WrapExecutionResult | null> {
  const [unwrapOut, wrapOut] = await Promise.all([
    deps.readUnwrapResult(requestId),
    deps.readWrapResult(requestId),
  ]);

  const [wrapValue] = wrapOut;
  if (wrapValue) {
    return {
      status: decodeExecutionStatus(wrapValue.status),
      ledgerTxId:
        wrapValue.pull_ledger_tx_id.length === 0 ? null : wrapValue.pull_ledger_tx_id[0],
      errorCode: wrapValue.error_code.length === 0 ? null : wrapValue.error_code[0],
      mintFailedRecoverable: wrapValue.mint_failed_recoverable,
      withdrawn: wrapValue.withdrawn,
      withdrawLedgerTxId:
        wrapValue.withdraw_ledger_tx_id.length === 0
          ? null
          : wrapValue.withdraw_ledger_tx_id[0],
      withdrawErrorCode:
        wrapValue.withdraw_error_code.length === 0 ? null : wrapValue.withdraw_error_code[0],
    };
  }

  const [unwrapValue] = unwrapOut;
  if (!unwrapValue) {
    return null;
  }
  return {
    status: decodeExecutionStatus(unwrapValue.status),
    ledgerTxId: unwrapValue.ledger_tx_id.length === 0 ? null : unwrapValue.ledger_tx_id[0],
    errorCode: unwrapValue.error_code.length === 0 ? null : unwrapValue.error_code[0],
    mintFailedRecoverable: false,
    withdrawn: false,
    withdrawLedgerTxId: null,
    withdrawErrorCode: null,
  };
}

export async function withdrawFailedWrap(
  requestId: Uint8Array,
  identity: Identity,
): Promise<{ requestId: Uint8Array; ledgerTxId: Uint8Array }> {
  const out = await (await getSubmitActor(identity)).withdraw_failed_wrap({
    request_id: requestId,
  });
  if ("Err" in out) {
    throw new Error(out.Err);
  }
  return {
    requestId: out.Ok.request_id,
    ledgerTxId: out.Ok.ledger_tx_id,
  };
}

export async function retryFailedUnwrap(
  requestId: Uint8Array,
  identity: Identity,
): Promise<Uint8Array> {
  const out = await (await getSubmitActor(identity)).retry_failed_unwrap({
    request_id: requestId,
  });
  if ("Err" in out) {
    throw new Error(out.Err);
  }
  return out.Ok.request_id;
}

export async function submitWrapRequest(
  args: {
    requestId: Uint8Array;
    assetId: Uint8Array;
    amount: Uint8Array;
    evmRecipient: Uint8Array;
    evmNonce: bigint;
    gasLimit: bigint;
  },
  identity: Identity,
): Promise<Uint8Array> {
  const out = await (await getSubmitActor(identity)).submit_wrap_request({
    request_id: args.requestId,
    asset_id: args.assetId,
    amount: args.amount,
    evm_recipient: args.evmRecipient,
    evm_nonce: args.evmNonce,
    gas_limit: args.gasLimit,
  });
  if ("Err" in out) {
    throw new Error(out.Err);
  }
  return out.Ok.request_id;
}

export async function getFeePolicy(): Promise<{
  feeLedgerCanister: string;
  cycleFeeE8s: bigint;
  gasPriceBufferBps: number;
}> {
  const out = await (await getQueryActor()).get_fee_policy();
  if ("Err" in out) {
    throw new Error(out.Err);
  }
  const value = out.Ok;
  return {
    feeLedgerCanister: value.fee_ledger_canister.toText(),
    cycleFeeE8s: value.cycle_fee_e8s,
    gasPriceBufferBps: value.gas_price_buffer_bps,
  };
}

export const wrapClientTestHooks = {
  reset(): void {
    cachedQueryActor = null;
    cachedSubmitActors.clear();
  },
};
