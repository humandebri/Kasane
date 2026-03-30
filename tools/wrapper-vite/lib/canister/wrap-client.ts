// どこで: wrap-canister クライアント / 何を: 新 wrap/unwrap API を UI 向けに公開する / なぜ: request_id 手組みや旧 status API を排除するため

import type { ActorSubclass, Identity } from "@icp-sdk/core/agent";
import { Principal } from "@icp-sdk/core/principal";
import { idlFactory as wrapIdlFactory } from "@/src/declarations/wrap_canister/wrap_canister.did.js";
import type {
  _SERVICE as WrapService,
  ApiError as ApiErrorWire,
  RequestKind as RequestKindVariant,
  RequestOverview as RequestOverviewWire,
  RequestStatus as RequestStatusVariant,
} from "@/src/declarations/wrap_canister/wrap_canister.did";
import { loadConfig, type WrapperConfig } from "../config";
import type { ExecutionStatus, WrapExecutionResult } from "../types";
import { createActorCache, createIdentityActor, createQueryActor } from "./actor-utils";

type WrapActor = ActorSubclass<WrapService>;
type PlainActorMethod<T> = T extends (...args: infer TArgs) => infer TResult ? (...args: TArgs) => TResult : never;

type ExecutionResultDeps = {
  readRequest: (requestId: Uint8Array) => Promise<[] | [RequestOverviewWire]>;
};
type QueryActorLike = {
  get_request: PlainActorMethod<WrapService["get_request"]>;
  quote_wrap_request: PlainActorMethod<WrapService["quote_wrap_request"]>;
  get_unwrap_requirements: PlainActorMethod<WrapService["get_unwrap_requirements"]>;
  get_fee_policy: PlainActorMethod<WrapService["get_fee_policy"]>;
};
type SubmitActorLike = {
  retry_request: PlainActorMethod<WrapService["retry_request"]>;
  recover_failed_wrap: PlainActorMethod<WrapService["recover_failed_wrap"]>;
  submit_wrap_request: PlainActorMethod<WrapService["submit_wrap_request"]>;
};

const actorCache = createActorCache<QueryActorLike, SubmitActorLike, WrapActor>();
type WrapClientDeps = {
  loadConfig: () => WrapperConfig;
};

const defaultWrapClientDeps: WrapClientDeps = {
  loadConfig,
};

let wrapClientDeps: WrapClientDeps = defaultWrapClientDeps;

async function getQueryActor(): Promise<QueryActorLike> {
  return actorCache.getQueryActor(async () => {
    const cfg = wrapClientDeps.loadConfig();
    return createQueryActor<WrapActor>({
      canisterId: cfg.wrapCanisterId,
      idlFactory: wrapIdlFactory,
    });
  });
}

async function getSubmitActor(identity: Identity): Promise<SubmitActorLike> {
  return actorCache.getSubmitActor(identity, async (nextIdentity) => {
    const cfg = wrapClientDeps.loadConfig();
    return createIdentityActor<WrapActor>({
      canisterId: cfg.wrapCanisterId,
      idlFactory: wrapIdlFactory,
      identity: nextIdentity,
    });
  });
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
      withdrawErrorCode: value.withdraw_error_code?.[0] ?? null,
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
  evmNonce: bigint;
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
    evm_nonce: args.evmNonce,
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
    actorCache.reset();
    wrapClientDeps = defaultWrapClientDeps;
  },
  setMockQueryActor(actor: QueryActorLike | null): void {
    actorCache.setMockQueryActor(actor);
  },
  setMockSubmitActor(actor: SubmitActorLike | null): void {
    actorCache.setMockSubmitActor(actor);
  },
  setDeps(deps: Partial<WrapClientDeps>): void {
    wrapClientDeps = {
      ...defaultWrapClientDeps,
      ...deps,
    };
  },
};
