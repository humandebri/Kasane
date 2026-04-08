// どこで: dashboard UI部品 / 何を: 画面専用の共有型を定義 / なぜ: 分割コンポーネント間の契約を明確にするため

import type { DispatchStatus, ExecutionStatus } from "@/lib/types";
import type { WalletSource } from "@/lib/wallet/types";

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

export type HistoryEntry = {
  requestId: string;
  kind: ActiveTab;
  submittedAt: string;
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
  session: {
    principalText: string;
    source: WalletSource;
  } | null;
  connecting: boolean;
  error: string | null;
};

export type StatusPanelView = {
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
