// どこで: BFFドメイン処理 / 何を: submit/status/health の中核処理を実装 / なぜ: route handlerからロジックを分離してテスト可能にするため

import { loadConfig } from "./config";
import { ApiError } from "./errors";
import { mergeStatus } from "./merge";
import { callerEvmAddressFromPrincipalText } from "./principal";
import { deriveRequestId, toSubmitIcTxData, WRAP_PRECOMPILE_ADDRESS } from "./request-id";
import { submitPrincipalTextFromSecretHex } from "./identity";
import type {
  DispatchStatus,
  HealthResponse,
  StatusResponse,
  SubmitPayload,
  SubmitResponse,
  WithdrawResponse,
} from "./types";
import { parseRequestIdHex, bytesToHex } from "./utils";
import { getDispatchResult, getDispatchStatus, getExpectedNonce, submitIcTx } from "./canister/wrapper-client";
import { getExecutionResult, withdrawFailedWrap } from "./canister/wrap-client";

export type SubmitDeps = {
  submitTx: typeof submitIcTx;
  readNonce: typeof getExpectedNonce;
  readDispatchStatus: typeof getDispatchStatus;
  makeUserNonce: () => bigint;
};

export type StatusDeps = {
  readDispatchStatus: typeof getDispatchStatus;
  readDispatchResult: typeof getDispatchResult;
  readExecutionResult: typeof getExecutionResult;
};

export type HealthDeps = {
  readDispatchStatus: typeof getDispatchStatus;
  readExecutionResult: typeof getExecutionResult;
};

export type WithdrawDeps = {
  withdrawFailedWrap: typeof withdrawFailedWrap;
};

export const defaultSubmitDeps: SubmitDeps = {
  submitTx: submitIcTx,
  readNonce: getExpectedNonce,
  readDispatchStatus: getDispatchStatus,
  makeUserNonce: randomU64Nonce,
};

export const defaultStatusDeps: StatusDeps = {
  readDispatchStatus: getDispatchStatus,
  readDispatchResult: getDispatchResult,
  readExecutionResult: getExecutionResult,
};

export const defaultHealthDeps: HealthDeps = {
  readDispatchStatus: getDispatchStatus,
  readExecutionResult: getExecutionResult,
};

export const defaultWithdrawDeps: WithdrawDeps = {
  withdrawFailedWrap,
};

function nowUnixSeconds(): bigint {
  return BigInt(Math.floor(Date.now() / 1000));
}

function randomU64Nonce(): bigint {
  const words = new Uint32Array(2);
  crypto.getRandomValues(words);
  const hi = words[0] ?? 0;
  const lo = words[1] ?? 0;
  return (BigInt(hi) << 32n) | BigInt(lo);
}

function deriveCallerPrincipalTextFromConfig(): string {
  const cfg = loadConfig();
  if (cfg.submitIdentitySecretKeyHex === null) {
    throw new ApiError(
      500,
      "config_missing",
      "config.missing:ICP_IDENTITY_SECRET_KEY_HEX"
    );
  }
  return submitPrincipalTextFromSecretHex(cfg.submitIdentitySecretKeyHex);
}

function defaultDispatchStatus(input: DispatchStatus | null): DispatchStatus {
  return input ?? "Queued";
}

export async function submitUnwrapRequest(payload: SubmitPayload, deps: SubmitDeps = defaultSubmitDeps): Promise<SubmitResponse> {
  const cfg = loadConfig();
  const callerPrincipalText = deriveCallerPrincipalTextFromConfig();
  const callerEvmAddress = callerEvmAddressFromPrincipalText(callerPrincipalText);
  const currentSec = nowUnixSeconds();
  const userNonce = deps.makeUserNonce();

  const data = toSubmitIcTxData({
    vaultCanisterId: cfg.wrapCanisterId,
    assetId: payload.assetId,
    amount: BigInt(payload.amount),
    recipient: payload.recipient,
    userNonce,
    deadline: currentSec + 3600n,
  });
  const requestIdBytes = deriveRequestId({
    callerEvmAddress,
    vaultCanisterId: cfg.wrapCanisterId,
    assetId: payload.assetId,
    amount: BigInt(payload.amount),
    recipient: payload.recipient,
    userNonce,
    deadline: currentSec + 3600n,
  });

  const nonce = await deps.readNonce(callerEvmAddress);
  await deps.submitTx({
    to: WRAP_PRECOMPILE_ADDRESS,
    data,
    nonce,
  });

  const dispatchStatus = defaultDispatchStatus(await deps.readDispatchStatus(requestIdBytes));
  return {
    ok: true,
    requestId: bytesToHex(requestIdBytes),
    dispatchStatus,
    vaultCanisterId: cfg.wrapCanisterId,
  };
}

export async function getRequestStatus(requestIdHex: string, deps: StatusDeps = defaultStatusDeps): Promise<StatusResponse> {
  const requestId = parseRequestIdHex(requestIdHex);
  const [dispatchStatus, dispatchResult, executionResult] = await Promise.all([
    deps.readDispatchStatus(requestId),
    deps.readDispatchResult(requestId),
    deps.readExecutionResult(requestId),
  ]);
  return mergeStatus({
    requestIdHex,
    dispatchStatus,
    dispatchResult,
    executionResult,
  });
}

export async function getHealth(deps: HealthDeps = defaultHealthDeps): Promise<HealthResponse> {
  const cfg = loadConfig();
  const probe = new Uint8Array(32);
  const [wrapperProbe, wrapProbe] = await Promise.allSettled([
    deps.readDispatchStatus(probe),
    deps.readExecutionResult(probe),
  ]);
  return {
    ok: wrapperProbe.status === "fulfilled" && wrapProbe.status === "fulfilled",
    evmGatewayReachable: wrapperProbe.status === "fulfilled",
    wrapReachable: wrapProbe.status === "fulfilled",
    config: {
      icHost: cfg.icHost,
      evmGatewayCanisterId: cfg.evmGatewayCanisterId,
      wrapCanisterId: cfg.wrapCanisterId,
    },
  };
}

export async function withdrawRequest(
  requestIdHex: string,
  deps: WithdrawDeps = defaultWithdrawDeps
): Promise<WithdrawResponse> {
  const requestId = parseRequestIdHex(requestIdHex);
  const out = await deps.withdrawFailedWrap(requestId);
  return {
    ok: true,
    requestId: bytesToHex(out.requestId),
    ledgerTxId: bytesToHex(out.ledgerTxId),
  };
}
