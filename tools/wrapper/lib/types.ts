// どこで: wrapperダッシュボード共通型 / 何を: API契約とドメイン型を定義 / なぜ: UI/BFF/canister境界で意味の混線を防ぐため

export type DispatchStatus = "Queued" | "Dispatching" | "Dispatched" | "DispatchFailed";
export type ExecutionStatus = "Queued" | "Running" | "Succeeded" | "Failed";

export type SubmitPayload = {
  assetId: string;
  amount: string;
  recipient: string;
};

export type SubmitResponse = {
  ok: true;
  requestId: string;
  dispatchStatus: DispatchStatus;
  vaultCanisterId: string;
};

export type StatusResponse = {
  requestId: string;
  dispatchStatus: DispatchStatus | null;
  executionStatus: ExecutionStatus | null;
  vaultCanisterId: string | null;
  ledgerTxId: string | null;
  errorCode: string | null;
  mintFailedRecoverable: boolean;
  withdrawn: boolean;
  withdrawLedgerTxId: string | null;
  withdrawErrorCode: string | null;
};

export type HealthResponse = {
  ok: boolean;
  kasaneEvmReachable: boolean;
  wrapReachable: boolean;
  config: {
    icHost: string;
    kasaneEvmCanisterId: string;
    wrapCanisterId: string;
  };
};

export type ApiErrorBody = {
  ok: false;
  errorCode: string;
  message: string;
};

export type DispatchResultView = {
  status: DispatchStatus;
  vaultCanisterId: Uint8Array;
  errorCode: string | null;
};

export type WrapExecutionResult = {
  status: ExecutionStatus;
  ledgerTxId: Uint8Array | null;
  errorCode: string | null;
  mintFailedRecoverable: boolean;
  withdrawn: boolean;
  withdrawLedgerTxId: Uint8Array | null;
  withdrawErrorCode: string | null;
};

export type StatusMergeInput = {
  requestIdHex: string;
  dispatchStatus: DispatchStatus | null;
  dispatchResult: DispatchResultView | null;
  executionResult: WrapExecutionResult | null;
};

export type WithdrawResponse = {
  ok: true;
  requestId: string;
  ledgerTxId: string;
};
