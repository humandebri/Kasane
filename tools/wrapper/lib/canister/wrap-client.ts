// どこで: wrap-canister クライアント / 何を: 新 wrap/unwrap API を UI 向けに公開する / なぜ: request_id 手組みや旧 status API を排除するため

import { Actor, type ActorSubclass, type Identity } from "@dfinity/agent";
import { IDL } from "@dfinity/candid";
import { Principal } from "@dfinity/principal";
import { loadConfig } from "../config";
import type { ExecutionStatus, WrapExecutionResult } from "../types";
import { getIdentityAgent, getQueryAgent } from "./agent";

type ApiErrorWire =
  | { InvalidArgument: { code: string; message: string } }
  | { Rejected: { code: string; message: string } }
  | { Internal: { code: string; message: string } };
type RequestStatusVariant = { Queued: null } | { Running: null } | { Succeeded: null } | { Failed: null };
type RequestKindVariant = { Wrap: null } | { Unwrap: null };
type RequestDispatchStatusVariant =
  | { Queued: null }
  | { Dispatching: null }
  | { Dispatched: null }
  | { DispatchFailed: null };
type RequestErrorViewWire = {
  code: string;
  message: string;
};
type RequestOverviewWire = {
  kind: RequestKindVariant;
  request_id: Uint8Array;
  status: RequestStatusVariant;
  error: [] | [RequestErrorViewWire];
  fee_ledger_tx_id: [] | [Uint8Array];
  pull_ledger_tx_id: [] | [Uint8Array];
  mint_tx_id: [] | [Uint8Array];
  withdraw_ledger_tx_id: [] | [Uint8Array];
  ledger_tx_id: [] | [Uint8Array];
  dispatch_status: [] | [RequestDispatchStatusVariant];
  dispatch_error: [] | [string];
  charged_fee_e8s: [] | [bigint];
  charged_gas_price_wei: [] | [bigint];
};
type FeePolicyViewWire = {
  fee_ledger_canister: Principal;
  cycle_fee_e8s: bigint;
  gas_price_buffer_bps: number;
};
type QuoteWrapRequestOkWire = {
  charged_fee_e8s: bigint;
  charged_gas_price_wei: bigint;
  cycle_fee_e8s: bigint;
  fee_ledger_canister: Principal;
};
type SubmitWrapRequestOkWire = {
  request_id: Uint8Array;
  charged_fee_e8s: bigint;
  charged_gas_price_wei: bigint;
  fee_ledger_tx_id: Uint8Array;
};
type GetUnwrapRequirementsOkWire = {
  factory_address: Uint8Array;
  wrapped_token_address: [] | [Uint8Array];
  balance: bigint;
  allowance: bigint;
  approve_required: boolean;
  readiness:
    | { Ready: null }
    | { TokenNotDeployed: null }
    | { InsufficientBalance: null }
    | { InsufficientAllowance: null };
};

type WrapActor = ActorSubclass<{
  get_request: (requestId: Uint8Array) => Promise<[] | [RequestOverviewWire]>;
  retry_request: (args: { request_id: Uint8Array }) => Promise<{ Ok: RequestOverviewWire } | { Err: ApiErrorWire }>;
  recover_failed_wrap: (args: { request_id: Uint8Array }) => Promise<{ Ok: RequestOverviewWire } | { Err: ApiErrorWire }>;
  submit_wrap_request: (args: {
    asset_id: Principal;
    amount_e8s: bigint;
    evm_recipient: Uint8Array;
    gas_limit: bigint;
  }) => Promise<{ Ok: SubmitWrapRequestOkWire } | { Err: ApiErrorWire }>;
  quote_wrap_request: (args: {
    asset_id: Principal;
    amount_e8s: bigint;
    evm_recipient: Uint8Array;
    gas_limit: bigint;
  }) => Promise<{ Ok: QuoteWrapRequestOkWire } | { Err: ApiErrorWire }>;
  get_unwrap_requirements: (args: {
    asset_id: Principal;
    amount_e8s: bigint;
    caller_evm_address: Uint8Array;
  }) => Promise<{ Ok: GetUnwrapRequirementsOkWire } | { Err: ApiErrorWire }>;
  get_fee_policy: () => Promise<{ Ok: FeePolicyViewWire } | { Err: string }>;
}>;

type ExecutionResultDeps = {
  readRequest: (requestId: Uint8Array) => Promise<[] | [RequestOverviewWire]>;
};

let cachedQueryActor: WrapActor | null = null;
const cachedSubmitActors = new Map<string, WrapActor>();

const wrapIdlFactory: IDL.InterfaceFactory = ({ IDL: I }) => {
  const ApiErrorDetail = I.Record({ code: I.Text, message: I.Text });
  const ApiError = I.Variant({
    InvalidArgument: ApiErrorDetail,
    Rejected: ApiErrorDetail,
    Internal: ApiErrorDetail,
  });
  const RequestStatus = I.Variant({
    Queued: I.Null,
    Running: I.Null,
    Succeeded: I.Null,
    Failed: I.Null,
  });
  const RequestDispatchStatus = I.Variant({
    Queued: I.Null,
    Dispatching: I.Null,
    Dispatched: I.Null,
    DispatchFailed: I.Null,
  });
  const RequestOverview = I.Record({
    kind: I.Variant({ Wrap: I.Null, Unwrap: I.Null }),
    request_id: I.Vec(I.Nat8),
    status: RequestStatus,
    error: I.Opt(I.Record({ code: I.Text, message: I.Text })),
    fee_ledger_tx_id: I.Opt(I.Vec(I.Nat8)),
    pull_ledger_tx_id: I.Opt(I.Vec(I.Nat8)),
    mint_tx_id: I.Opt(I.Vec(I.Nat8)),
    withdraw_ledger_tx_id: I.Opt(I.Vec(I.Nat8)),
    ledger_tx_id: I.Opt(I.Vec(I.Nat8)),
    dispatch_status: I.Opt(RequestDispatchStatus),
    dispatch_error: I.Opt(I.Text),
    charged_fee_e8s: I.Opt(I.Nat),
    charged_gas_price_wei: I.Opt(I.Nat),
  });
  const UnwrapReadiness = I.Variant({
    Ready: I.Null,
    TokenNotDeployed: I.Null,
    InsufficientBalance: I.Null,
    InsufficientAllowance: I.Null,
  });
  return I.Service({
    get_request: I.Func([I.Vec(I.Nat8)], [I.Opt(RequestOverview)], ["query"]),
    retry_request: I.Func([I.Record({ request_id: I.Vec(I.Nat8) })], [I.Variant({ Ok: RequestOverview, Err: ApiError })], []),
    recover_failed_wrap: I.Func([I.Record({ request_id: I.Vec(I.Nat8) })], [I.Variant({ Ok: RequestOverview, Err: ApiError })], []),
    submit_wrap_request: I.Func(
      [I.Record({
        asset_id: I.Principal,
        amount_e8s: I.Nat,
        evm_recipient: I.Vec(I.Nat8),
        gas_limit: I.Nat64,
      })],
      [I.Variant({
        Ok: I.Record({
          request_id: I.Vec(I.Nat8),
          charged_fee_e8s: I.Nat,
          charged_gas_price_wei: I.Nat,
          fee_ledger_tx_id: I.Vec(I.Nat8),
        }),
        Err: ApiError,
      })],
      []
    ),
    quote_wrap_request: I.Func(
      [I.Record({
        asset_id: I.Principal,
        amount_e8s: I.Nat,
        evm_recipient: I.Vec(I.Nat8),
        gas_limit: I.Nat64,
      })],
      [I.Variant({
        Ok: I.Record({
          charged_fee_e8s: I.Nat,
          charged_gas_price_wei: I.Nat,
          cycle_fee_e8s: I.Nat64,
          fee_ledger_canister: I.Principal,
        }),
        Err: ApiError,
      })],
      ["query"]
    ),
    get_unwrap_requirements: I.Func(
      [I.Record({
        asset_id: I.Principal,
        amount_e8s: I.Nat,
        caller_evm_address: I.Vec(I.Nat8),
      })],
      [I.Variant({
        Ok: I.Record({
          factory_address: I.Vec(I.Nat8),
          wrapped_token_address: I.Opt(I.Vec(I.Nat8)),
          balance: I.Nat,
          allowance: I.Nat,
          approve_required: I.Bool,
          readiness: UnwrapReadiness,
        }),
        Err: ApiError,
      })],
      ["query"]
    ),
    get_fee_policy: I.Func([], [I.Variant({ Ok: I.Record({
      fee_ledger_canister: I.Principal,
      cycle_fee_e8s: I.Nat64,
      gas_price_buffer_bps: I.Nat32,
    }), Err: I.Text })], ["query"]),
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

function decodeExecutionStatus(status: RequestStatusVariant): ExecutionStatus {
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

function decodeApiError(err: ApiErrorWire): string {
  if ("InvalidArgument" in err) {
    return `${err.InvalidArgument.code}:${err.InvalidArgument.message}`;
  }
  if ("Rejected" in err) {
    return `${err.Rejected.code}:${err.Rejected.message}`;
  }
  return `${err.Internal.code}:${err.Internal.message}`;
}

function isWrapRequest(value: RequestOverviewWire): boolean {
  return "Wrap" in value.kind;
}

function inferMintRecoverable(value: RequestOverviewWire): boolean {
  return (
    isWrapRequest(value) &&
    "Failed" in value.status &&
    value.pull_ledger_tx_id.length > 0 &&
    value.mint_tx_id.length === 0 &&
    value.withdraw_ledger_tx_id.length === 0
  );
}

const defaultExecutionResultDeps: ExecutionResultDeps = {
  readRequest: async (requestId: Uint8Array) => (await getQueryActor()).get_request(requestId),
};

export async function getExecutionResult(
  requestId: Uint8Array,
  deps: ExecutionResultDeps = defaultExecutionResultDeps,
): Promise<WrapExecutionResult | null> {
  const [value] = await deps.readRequest(requestId);
  if (!value) {
    return null;
  }
  return {
    status: decodeExecutionStatus(value.status),
    ledgerTxId: value.ledger_tx_id[0] ?? value.pull_ledger_tx_id[0] ?? null,
    errorCode: value.error[0]?.code ?? null,
    mintFailedRecoverable: inferMintRecoverable(value),
    withdrawn: value.withdraw_ledger_tx_id.length > 0,
    withdrawLedgerTxId: value.withdraw_ledger_tx_id[0] ?? null,
    withdrawErrorCode: null,
  };
}

export async function withdrawFailedWrap(
  requestId: Uint8Array,
  identity: Identity,
): Promise<{ requestId: Uint8Array; ledgerTxId: Uint8Array }> {
  const out = await (await getSubmitActor(identity)).recover_failed_wrap({ request_id: requestId });
  if ("Err" in out) {
    throw new Error(decodeApiError(out.Err));
  }
  const ledgerTxId = out.Ok.withdraw_ledger_tx_id[0];
  if (ledgerTxId === undefined) {
    throw new Error("wrap.recover_missing_withdraw_ledger_tx_id");
  }
  return {
    requestId: out.Ok.request_id,
    ledgerTxId,
  };
}

export async function retryFailedUnwrap(
  requestId: Uint8Array,
  identity: Identity,
): Promise<Uint8Array> {
  const out = await (await getSubmitActor(identity)).retry_request({ request_id: requestId });
  if ("Err" in out) {
    throw new Error(decodeApiError(out.Err));
  }
  return out.Ok.request_id;
}

export async function submitWrapRequest(args: {
  assetId: string;
  amountE8s: bigint;
  evmRecipient: Uint8Array;
  gasLimit: bigint;
}, identity: Identity): Promise<{
  requestId: Uint8Array;
  chargedFeeE8s: bigint;
  chargedGasPriceWei: bigint;
  feeLedgerTxId: Uint8Array;
}> {
  const out = await (await getSubmitActor(identity)).submit_wrap_request({
    asset_id: Principal.fromText(args.assetId),
    amount_e8s: args.amountE8s,
    evm_recipient: args.evmRecipient,
    gas_limit: args.gasLimit,
  });
  if ("Err" in out) {
    throw new Error(decodeApiError(out.Err));
  }
  return {
    requestId: out.Ok.request_id,
    chargedFeeE8s: out.Ok.charged_fee_e8s,
    chargedGasPriceWei: out.Ok.charged_gas_price_wei,
    feeLedgerTxId: out.Ok.fee_ledger_tx_id,
  };
}

export async function quoteWrapRequest(args: {
  assetId: string;
  amountE8s: bigint;
  evmRecipient: Uint8Array;
  gasLimit: bigint;
}): Promise<{
  chargedFeeE8s: bigint;
  chargedGasPriceWei: bigint;
  cycleFeeE8s: bigint;
  feeLedgerCanister: string;
}> {
  const out = await (await getQueryActor()).quote_wrap_request({
    asset_id: Principal.fromText(args.assetId),
    amount_e8s: args.amountE8s,
    evm_recipient: args.evmRecipient,
    gas_limit: args.gasLimit,
  });
  if ("Err" in out) {
    throw new Error(decodeApiError(out.Err));
  }
  return {
    chargedFeeE8s: out.Ok.charged_fee_e8s,
    chargedGasPriceWei: out.Ok.charged_gas_price_wei,
    cycleFeeE8s: out.Ok.cycle_fee_e8s,
    feeLedgerCanister: out.Ok.fee_ledger_canister.toText(),
  };
}

export async function getUnwrapRequirements(args: {
  assetId: string;
  amountE8s: bigint;
  callerEvmAddress: Uint8Array;
}): Promise<{
  factoryAddress: Uint8Array;
  wrappedTokenAddress: Uint8Array | null;
  balance: bigint;
  allowance: bigint;
  approveRequired: boolean;
  readiness: "Ready" | "TokenNotDeployed" | "InsufficientBalance" | "InsufficientAllowance";
}> {
  const out = await (await getQueryActor()).get_unwrap_requirements({
    asset_id: Principal.fromText(args.assetId),
    amount_e8s: args.amountE8s,
    caller_evm_address: args.callerEvmAddress,
  });
  if ("Err" in out) {
    throw new Error(decodeApiError(out.Err));
  }
  const readiness = "Ready" in out.Ok.readiness
    ? "Ready"
    : "TokenNotDeployed" in out.Ok.readiness
      ? "TokenNotDeployed"
      : "InsufficientBalance" in out.Ok.readiness
        ? "InsufficientBalance"
        : "InsufficientAllowance";
  return {
    factoryAddress: out.Ok.factory_address,
    wrappedTokenAddress: out.Ok.wrapped_token_address[0] ?? null,
    balance: out.Ok.balance,
    allowance: out.Ok.allowance,
    approveRequired: out.Ok.approve_required,
    readiness,
  };
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
  return {
    feeLedgerCanister: out.Ok.fee_ledger_canister.toText(),
    cycleFeeE8s: out.Ok.cycle_fee_e8s,
    gasPriceBufferBps: out.Ok.gas_price_buffer_bps,
  };
}

export const wrapClientTestHooks = {
  reset(): void {
    cachedQueryActor = null;
    cachedSubmitActors.clear();
  },
};
