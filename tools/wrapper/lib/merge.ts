// どこで: status統合ロジック / 何を: dispatch/executionの2系統状態を1レスポンスへ統合 / なぜ: 表示責務をBFF側に集約するため

import { Principal } from "@dfinity/principal";
import type { StatusMergeInput, StatusResponse } from "./types";
import { bytesToHex } from "./utils";

function principalBytesToText(bytes: Uint8Array): string | null {
  if (bytes.length === 0) {
    return null;
  }
  try {
    return Principal.fromUint8Array(bytes).toText();
  } catch {
    return null;
  }
}

export function mergeStatus(input: StatusMergeInput): StatusResponse {
  const dispatchStatus = input.dispatchResult?.status ?? input.dispatchStatus;
  const vaultCanisterId = input.dispatchResult
    ? principalBytesToText(input.dispatchResult.vaultCanisterId)
    : null;
  const executionStatus = input.executionResult?.status ?? null;
  const ledgerTxId = input.executionResult?.ledgerTxId
    ? bytesToHex(input.executionResult.ledgerTxId)
    : null;
  const errorCode =
    input.dispatchResult?.errorCode ??
    input.executionResult?.errorCode ??
    null;
  const mintFailedRecoverable =
    input.executionResult?.mintFailedRecoverable ?? false;
  const withdrawn = input.executionResult?.withdrawn ?? false;
  const withdrawLedgerTxId = input.executionResult?.withdrawLedgerTxId
    ? bytesToHex(input.executionResult.withdrawLedgerTxId)
    : null;
  const withdrawErrorCode = input.executionResult?.withdrawErrorCode ?? null;

  return {
    requestId: input.requestIdHex,
    dispatchStatus,
    executionStatus,
    vaultCanisterId,
    ledgerTxId,
    errorCode,
    mintFailedRecoverable,
    withdrawn,
    withdrawLedgerTxId,
    withdrawErrorCode,
  };
}
