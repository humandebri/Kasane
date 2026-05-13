// どこで: dashboard UI部品 / 何を: 画面専用の共有型を定義 / なぜ: Oisy と MetaMask の表示契約を明確に保つため

import type { DispatchStatus, ExecutionStatus } from "@/lib/types";
import type { KasaneTransactionStatus } from "@/lib/kasane-rpc";
import type { MetaMaskSession, WalletSession } from "@/lib/wallet/types";

type DashboardOisyCapabilities = {
  ledgerApproveSupported: boolean;
  wrapCanisterSupported: boolean;
  gatewaySupported: boolean;
};

export type ActiveTab = "unwrap" | "wrap";

export type UnwrapFormState = {
  assetId: string;
  amount: string;
  recipient: string;
};

export type WrapFormState = {
  assetId: string;
  amount: string;
  evmRecipient: string;
  evmNonce: string;
  gasLimit: string;
};

export type WrapActionStep =
  | "idle"
  | "quoting"
  | "checking_allowance"
  | "approving_asset"
  | "approving_fee"
  | "submitting"
  | "done"
  | "error";

export type WrapGasEstimateStatus =
  | "idle"
  | "estimating"
  | "ready"
  | "error";

export type WrapNonceStatus =
  | "idle"
  | "loading"
  | "ready"
  | "error";

export type DashboardWalletState = {
  oisySession: WalletSession | null;
  metaMaskSession: MetaMaskSession | null;
  oisyConnecting: boolean;
  metaMaskConnecting: boolean;
  metaMaskAvailable: boolean;
  oisyCapabilities: DashboardOisyCapabilities;
  error: string | null;
};

export type RequestStatusPanelView = {
  kind: "request";
  requestId: string;
  dispatchStatus: DispatchStatus | null;
  executionStatus: ExecutionStatus | null;
  ledgerTxId: string | null;
  errorCode: string | null;
  mintFailedRecoverable: boolean;
  withdrawn: boolean;
  withdrawLedgerTxId: string | null;
  withdrawErrorCode: string | null;
};

export type TransactionStatusPanelView = KasaneTransactionStatus & {
  kind: "transaction";
};

export type StatusPanelView = RequestStatusPanelView | TransactionStatusPanelView;
