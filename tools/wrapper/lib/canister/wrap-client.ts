// どこで: wrap-canister クライアント / 何を: execution状態を取得 / なぜ: dispatchと最終実行結果を分離表示するため

import { Actor, type ActorSubclass } from "@dfinity/agent";
import { IDL } from "@dfinity/candid";
import { loadConfig } from "../config";
import type { ExecutionStatus, WrapExecutionResult } from "../types";
import { getQueryAgent, getSubmitAgent } from "./agent";

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
};

type WrapActor = ActorSubclass<{
  get_request_result: (requestId: Uint8Array) => Promise<[] | [UnwrapRequestResultWire]>;
  get_wrap_request_result: (requestId: Uint8Array) => Promise<[] | [WrapRequestResultWire]>;
  withdraw_failed_wrap: (args: { request_id: Uint8Array }) => Promise<
    | { Ok: { request_id: Uint8Array; ledger_tx_id: Uint8Array } }
    | { Err: string }
  >
}>;

type ExecutionResultDeps = {
  readUnwrapResult: (requestId: Uint8Array) => Promise<[] | [UnwrapRequestResultWire]>;
  readWrapResult: (requestId: Uint8Array) => Promise<[] | [WrapRequestResultWire]>;
};

let cachedQueryActor: WrapActor | null = null;
let cachedSubmitActor: WrapActor | null = null;

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
  });
  const WithdrawFailedWrapArgs = I.Record({
    request_id: I.Vec(I.Nat8),
  });
  const WithdrawFailedWrapOk = I.Record({
    request_id: I.Vec(I.Nat8),
    ledger_tx_id: I.Vec(I.Nat8),
  });
  return I.Service({
    get_request_result: I.Func([I.Vec(I.Nat8)], [I.Opt(RequestResult)], ["query"]),
    get_wrap_request_result: I.Func([I.Vec(I.Nat8)], [I.Opt(WrapRequestResult)], ["query"]),
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

async function getSubmitActor(): Promise<WrapActor> {
  if (cachedSubmitActor) {
    return cachedSubmitActor;
  }
  const cfg = loadConfig();
  cachedSubmitActor = Actor.createActor<WrapActor>(wrapIdlFactory, {
    canisterId: cfg.wrapCanisterId,
    agent: await getSubmitAgent(),
  });
  return cachedSubmitActor;
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
  requestId: Uint8Array
): Promise<{ requestId: Uint8Array; ledgerTxId: Uint8Array }> {
  const out = await (await getSubmitActor()).withdraw_failed_wrap({
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

export const wrapClientTestHooks = {
  reset(): void {
    cachedQueryActor = null;
    cachedSubmitActor = null;
  },
};
