// どこで: wrapper dashboard hook / 何を: unwrap/wrap/withdraw送信処理を提供 / なぜ: 画面コンポーネントから副作用ロジックを分離するため

import { useCallback, useEffect, useState } from "react";
import type { Identity } from "@icp-sdk/core/agent";
import type {
  HistoryEntry,
  UnwrapFormState,
  WrapActionStep,
  WrapFormState,
} from "@/components/dashboard-ui/types";
import { approveLedgerSpend, getLedgerAllowance } from "@/lib/canister/icrc2-client";
import { approveWrappedTokenIfNeeded } from "@/lib/canister/erc20-client";
import {
  estimateIcTx,
  getExpectedNonce,
  getMaxPriorityFeePerGasWei,
  getUnwrapRequestIdsByTxId,
  submitIcTx,
} from "@/lib/canister/wrapper-client";
import {
  quoteWrapRequest,
  retryFailedUnwrap,
  submitWrapRequest,
  withdrawFailedWrap,
} from "@/lib/canister/wrap-client";
import type { loadConfig } from "@/lib/config";
import { callerEvmAddressFromPrincipalText } from "@/lib/principal";
import {
  toSubmitIcTxData,
  WRAP_PRECOMPILE_ADDRESS,
} from "@/lib/request-id";
import { bytesToHex, hexToBytes, parseRequestIdHex } from "@/lib/utils";
import {
  computeRequiredAllowances,
  formatE8sToIcpText4,
} from "@/lib/wrap-flow";
import { parsePositiveU64, parseTokenAmount, parseU64 } from "@/lib/wrap-input";
import type { WalletSession } from "@/lib/wallet/types";

type AppConfig = ReturnType<typeof loadConfig>;

type StatusTrackerState = {
  status: { requestId: string } | null;
  setStatus: (value: {
    requestId: string;
    dispatchStatus: null;
    executionStatus: null;
    ledgerTxId: null;
    errorCode: null;
    mintFailedRecoverable: false;
    withdrawn: false;
    withdrawLedgerTxId: null;
    withdrawErrorCode: null;
  } | null) => void;
  setMessage: (value: string | null) => void;
  refreshStatus: (requestIdHex: string, background?: boolean) => Promise<boolean>;
  setAutoPolling: (value: boolean) => void;
};

type WrapperFormsState = {
  unwrapForm: UnwrapFormState;
  wrapForm: WrapFormState;
  wrapPreviewRequestId: string | null;
  unwrapAssetDecimals: number | null;
  unwrapAssetDecimalsError: string | null;
  wrapAssetDecimals: number | null;
  wrapAssetDecimalsError: string | null;
  wrapGasEstimateStatus: "idle" | "estimating" | "ready" | "error";
  wrapGasEstimateError: string | null;
  wrapNonceStatus: "idle" | "loading" | "ready" | "error";
  wrapNonceError: string | null;
  resetUnwrapNonceDeadline: () => void;
  refreshWrapNonce: () => Promise<void>;
};

function persistSubmittedRequest(
  onRequestSubmitted: (entry: HistoryEntry) => Promise<void> | void,
  entry: HistoryEntry,
): void {
  void Promise.resolve(onRequestSubmitted(entry)).catch(() => undefined);
}

export function useWrapperActions(params: {
  cfg: AppConfig | null;
  configError: string | null;
  walletSession: WalletSession | null;
  getIdentity: () => Promise<Identity | null>;
  forms: WrapperFormsState;
  tracker: StatusTrackerState;
  onRequestSubmitted: (entry: HistoryEntry) => Promise<void> | void;
  onRequestIdInput: (requestId: string) => void;
  onWrapActionStepChange: (step: WrapActionStep) => void;
}) {
  const [submitLoading, setSubmitLoading] = useState(false);
  const [retryLoading, setRetryLoading] = useState(false);
  const [withdrawLoading, setWithdrawLoading] = useState(false);
  const [wrapActionStep, setWrapActionStep] = useState<WrapActionStep>("idle");
  const [wrapFeeEstimateText, setWrapFeeEstimateText] = useState<string | null>(null);
  const [wrapFeeEstimate, setWrapFeeEstimate] = useState<{
    chargedFeeE8s: bigint;
    feeLedgerCanister: string;
  } | null>(null);
  const [wrapGasDetails, setWrapGasDetails] = useState<{
    chargedGasPriceWei: bigint;
    maxPriorityFeePerGasWei: bigint;
  } | null>(null);
  const [lastSubmittedWrapRequestId, setLastSubmittedWrapRequestId] = useState<string | null>(null);

  function updateWrapActionStep(step: WrapActionStep): void {
    setWrapActionStep(step);
    params.onWrapActionStepChange(step);
  }

  useEffect(() => {
    setLastSubmittedWrapRequestId((current) => (current === null ? current : null));
    if (wrapActionStep !== "idle") {
      updateWrapActionStep("idle");
    }
  }, [
    params.forms.wrapForm.assetId,
    params.forms.wrapForm.amount,
    params.forms.wrapForm.evmRecipient,
  ]);

  function requireReady(): { cfg: AppConfig; principalText: string } | null {
    if (!params.cfg) {
      params.tracker.setMessage(params.configError ?? "config.invalid");
      return null;
    }
    if (!params.walletSession) {
      params.tracker.setMessage("wallet.not_connected");
      return null;
    }
    return { cfg: params.cfg, principalText: params.walletSession.principalText };
  }

  async function requireIdentity(): Promise<Identity> {
    const identity = await params.getIdentity();
    if (identity === null) {
      throw new Error("wallet.not_connected");
    }
    return identity;
  }

  const queryAndStartPolling = useCallback(async (trackingIdHex: string): Promise<void> => {
    const ok = await params.tracker.refreshStatus(trackingIdHex);
    if (ok) {
      params.tracker.setAutoPolling(true);
    }
  }, [params.tracker]);

  const startPollingSubmittedRequest = useCallback(async (requestIdHex: string): Promise<void> => {
    params.tracker.setStatus({
      requestId: requestIdHex,
      dispatchStatus: null,
      executionStatus: null,
      ledgerTxId: null,
      errorCode: null,
      mintFailedRecoverable: false,
      withdrawn: false,
      withdrawLedgerTxId: null,
      withdrawErrorCode: null,
    });
    params.tracker.setAutoPolling(true);
    await params.tracker.refreshStatus(requestIdHex, true);
  }, [params.tracker]);

  async function resolveUnwrapRequestIdHex(txId: Uint8Array): Promise<string> {
    for (let attempt = 0; attempt < 20; attempt += 1) {
      const requestIds = await getUnwrapRequestIdsByTxId(txId);
      if (requestIds.length === 1) {
        const requestId = requestIds[0];
        if (requestId !== undefined) {
          return bytesToHex(requestId);
        }
      }
      if (requestIds.length > 1) {
        throw new Error("status.unwrap_tx.multiple_request_ids");
      }
      await new Promise((resolve) => {
        globalThis.setTimeout(resolve, 500);
      });
    }
    throw new Error("status.unwrap_tx.request_id_unresolved");
  }

  async function submitUnwrap(): Promise<void> {
    const ready = requireReady();
    if (!ready) {
      return;
    }
    try {
      setSubmitLoading(true);
      params.tracker.setMessage(null);
      if (params.forms.unwrapForm.assetId.trim() === "") {
        throw new Error("validation.asset_id_required");
      }
      if (params.forms.unwrapForm.recipient.trim() === "") {
        throw new Error("validation.recipient_required");
      }
      if (params.forms.unwrapAssetDecimals === null) {
        throw new Error(params.forms.unwrapAssetDecimalsError ?? "wrap.asset_metadata_failed");
      }
      const amount = parseTokenAmount(
        params.forms.unwrapForm.amount,
        params.forms.unwrapAssetDecimals,
        "validation.amount.invalid",
      );
      const identity = await requireIdentity();
      await approveWrappedTokenIfNeeded({
        assetId: params.forms.unwrapForm.assetId.trim(),
        amount,
        principalText: ready.principalText,
        identity,
      });
      const callerEvmAddress = callerEvmAddressFromPrincipalText(ready.principalText);
      const txData = toSubmitIcTxData({
        assetId: params.forms.unwrapForm.assetId.trim(),
        amount,
        recipient: params.forms.unwrapForm.recipient.trim(),
      });
      const nonce = await getExpectedNonce(callerEvmAddress);
      const estimate = await estimateIcTx({
        from: callerEvmAddress,
        to: WRAP_PRECOMPILE_ADDRESS,
        data: txData,
        nonce,
        gasLimit: 300_000n,
      });
      const txId = await submitIcTx({
        to: WRAP_PRECOMPILE_ADDRESS,
        data: txData,
        nonce,
        gasLimit: estimate.gasLimit,
        identity,
        maxFeePerGas: estimate.suggestedMaxFeePerGas,
        maxPriorityFeePerGas: estimate.suggestedMaxPriorityFeePerGas,
      });
      const requestIdHex = await resolveUnwrapRequestIdHex(txId);
      params.onRequestIdInput(requestIdHex);
      await startPollingSubmittedRequest(requestIdHex);
      persistSubmittedRequest(params.onRequestSubmitted, {
        requestId: requestIdHex,
        kind: "unwrap",
        submittedAt: new Date().toISOString(),
      });
      params.tracker.setMessage("submit.success");
      params.forms.resetUnwrapNonceDeadline();
    } catch (error) {
      params.tracker.setMessage(error instanceof Error ? error.message : "submit_failed");
    } finally {
      setSubmitLoading(false);
    }
  }

  async function submitWrap(): Promise<void> {
    const ready = requireReady();
    if (!ready) {
      return;
    }
    try {
      setSubmitLoading(true);
      params.tracker.setMessage(null);
      updateWrapActionStep("quoting");
      if (params.forms.wrapForm.assetId.trim() === "") {
        throw new Error("validation.asset_id_required");
      }
      if (params.forms.wrapForm.evmRecipient.trim() === "") {
        throw new Error("validation.evm_recipient_required");
      }
      if (params.forms.wrapAssetDecimals === null) {
        throw new Error(params.forms.wrapAssetDecimalsError ?? "wrap.asset_metadata_failed");
      }
      const amount = parseTokenAmount(
        params.forms.wrapForm.amount,
        params.forms.wrapAssetDecimals,
        "validation.amount.invalid",
      );
      if (params.forms.wrapGasEstimateStatus !== "ready") {
        throw new Error(params.forms.wrapGasEstimateError ?? "wrap.gas_estimate_failed");
      }
      const gasLimit = parsePositiveU64(
        params.forms.wrapForm.gasLimit,
        "validation.gas_limit.invalid",
      );
      const evmNonce = parseU64(
        params.forms.wrapForm.evmNonce,
        "validation.evm_nonce.invalid",
      );
      const quote = await quoteWrapRequest({
        assetId: params.forms.wrapForm.assetId.trim(),
        amountE8s: amount,
        evmRecipient: hexToBytes(params.forms.wrapForm.evmRecipient.trim()),
        gasLimit,
      });
      setWrapFeeEstimate({
        chargedFeeE8s: quote.chargedFeeE8s,
        feeLedgerCanister: quote.feeLedgerCanister,
      });
      const priorityFeeWei = await getMaxPriorityFeePerGasWei().catch(() => null);
      setWrapGasDetails(priorityFeeWei === null ? null : {
        chargedGasPriceWei: quote.chargedGasPriceWei,
        maxPriorityFeePerGasWei: priorityFeeWei,
      });
      setWrapFeeEstimateText(
        `estimated fee: ${formatE8sToIcpText4(quote.chargedFeeE8s)} ICP`,
      );

      updateWrapActionStep("checking_allowance");
      const required = computeRequiredAllowances({
        assetLedgerCanister: params.forms.wrapForm.assetId.trim(),
        feeLedgerCanister: quote.feeLedgerCanister,
        amount,
        totalFeeE8s: quote.chargedFeeE8s,
      });
      const ownerPrincipalText = ready.principalText;
      const spenderCanisterId = ready.cfg.wrapCanisterId.trim();
      const identity = await requireIdentity();
      const assetAllowance = await getLedgerAllowance({
        ledgerCanisterId: params.forms.wrapForm.assetId.trim(),
        ownerPrincipalText,
        spenderCanisterId,
      });
      if (assetAllowance < required.requiredAssetAllowance) {
        updateWrapActionStep("approving_asset");
          await approveLedgerSpend({
            ledgerCanisterId: params.forms.wrapForm.assetId.trim(),
            spenderCanisterId,
            amount: required.requiredAssetAllowance,
            identity,
          });
      }
      if (required.requiredFeeAllowance > 0n) {
        const feeAllowance = await getLedgerAllowance({
          ledgerCanisterId: quote.feeLedgerCanister,
          ownerPrincipalText,
          spenderCanisterId,
        });
        if (feeAllowance < required.requiredFeeAllowance) {
          updateWrapActionStep("approving_fee");
          await approveLedgerSpend({
            ledgerCanisterId: quote.feeLedgerCanister,
            spenderCanisterId,
            amount: required.requiredFeeAllowance,
            identity,
          });
        }
      }

      updateWrapActionStep("submitting");
      const submitResult = await submitWrapRequest({
        assetId: params.forms.wrapForm.assetId.trim(),
        amountE8s: amount,
        evmRecipient: hexToBytes(params.forms.wrapForm.evmRecipient.trim()),
        evmNonce,
        gasLimit,
      }, identity);
      const requestIdHex = bytesToHex(submitResult.requestId);
      setLastSubmittedWrapRequestId(requestIdHex);
      params.onRequestIdInput(requestIdHex);
      await startPollingSubmittedRequest(requestIdHex);
      persistSubmittedRequest(params.onRequestSubmitted, {
        requestId: requestIdHex,
        kind: "wrap",
        submittedAt: new Date().toISOString(),
      });
      await params.forms.refreshWrapNonce().catch(() => undefined);
      updateWrapActionStep("done");
      params.tracker.setMessage(`wrap.submit.success fee=${submitResult.chargedFeeE8s.toString()}e8s`);
    } catch (error) {
      updateWrapActionStep("error");
      if (
        error instanceof Error &&
        error.message.startsWith("wrap.request.duplicate") &&
        params.forms.wrapGasEstimateStatus === "ready"
      ) {
        const duplicateRequestId =
          params.forms.wrapPreviewRequestId
          ?? params.tracker.status?.requestId
          ?? lastSubmittedWrapRequestId
          ?? null;
        if (duplicateRequestId) {
          setLastSubmittedWrapRequestId(duplicateRequestId);
          params.onRequestIdInput(duplicateRequestId);
          await startPollingSubmittedRequest(duplicateRequestId).catch(() => undefined);
          params.tracker.setMessage("wrap.request.duplicate_existing_request_loaded");
          return;
        }
      }
      params.tracker.setMessage(
        error instanceof Error ? error.message : "wrap_submit_failed",
      );
    } finally {
      setSubmitLoading(false);
    }
  }

  useEffect(() => {
    if (params.forms.wrapGasEstimateStatus !== "ready") {
      setWrapFeeEstimate(null);
      setWrapGasDetails(null);
      setWrapFeeEstimateText(null);
      return;
    }
    let cancelled = false;
    const assetId = params.forms.wrapForm.assetId.trim();
    const amountText = params.forms.wrapForm.amount.trim();
    const evmRecipient = params.forms.wrapForm.evmRecipient.trim();
    if (assetId === "" || amountText === "" || evmRecipient === "") {
      if (evmRecipient === "") {
        setWrapFeeEstimate(null);
        setWrapGasDetails(null);
        setWrapFeeEstimateText(null);
        return;
      }
    }
    let amountE8s: bigint;
    let gasLimit: bigint;
    let evmRecipientBytes: Uint8Array;
    try {
      if (params.forms.wrapAssetDecimals === null) {
        setWrapFeeEstimate(null);
        setWrapGasDetails(null);
        setWrapFeeEstimateText(null);
        return;
      }
      amountE8s = amountText === ""
        ? 1n
        : parseTokenAmount(amountText, params.forms.wrapAssetDecimals, "validation.amount.invalid");
      gasLimit = parsePositiveU64(
        params.forms.wrapForm.gasLimit,
        "validation.gas_limit.invalid",
      );
      evmRecipientBytes = hexToBytes(evmRecipient);
    } catch {
      setWrapFeeEstimate(null);
      setWrapGasDetails(null);
      setWrapFeeEstimateText(null);
      return;
    }
    void Promise.all([
      quoteWrapRequest({
        assetId,
        amountE8s,
        evmRecipient: evmRecipientBytes,
        gasLimit,
      }),
      getMaxPriorityFeePerGasWei(),
    ])
      .then(([quote, priorityFeeWei]) => {
        if (cancelled) {
          return;
        }
        setWrapFeeEstimate({
          chargedFeeE8s: quote.chargedFeeE8s,
          feeLedgerCanister: quote.feeLedgerCanister,
        });
        setWrapGasDetails({
          chargedGasPriceWei: quote.chargedGasPriceWei,
          maxPriorityFeePerGasWei: priorityFeeWei,
        });
        setWrapFeeEstimateText(
          `estimated fee: ${formatE8sToIcpText4(quote.chargedFeeE8s)} ICP`,
        );
      })
      .catch(() => {
        if (!cancelled) {
          setWrapFeeEstimate(null);
          setWrapGasDetails(null);
          setWrapFeeEstimateText(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    params.forms.wrapForm.assetId,
    params.forms.wrapForm.amount,
    params.forms.wrapForm.evmRecipient,
    params.forms.wrapForm.gasLimit,
    params.forms.wrapAssetDecimals,
    params.forms.wrapGasEstimateStatus,
  ]);

  async function withdraw(): Promise<void> {
    if (!params.walletSession || !params.tracker.status) {
      params.tracker.setMessage("status.not_loaded");
      return;
    }
    if (!params.cfg) {
      params.tracker.setMessage(params.configError ?? "config.invalid");
      return;
    }
    try {
      setWithdrawLoading(true);
      params.tracker.setMessage(null);
      const identity = await requireIdentity();
      await withdrawFailedWrap(
        parseRequestIdHex(params.tracker.status.requestId),
        identity,
      );
      await queryAndStartPolling(params.tracker.status.requestId);
      params.tracker.setMessage("withdraw.success");
    } catch (error) {
      params.tracker.setMessage(error instanceof Error ? error.message : "withdraw_failed");
    } finally {
      setWithdrawLoading(false);
    }
  }

  async function retryUnwrap(): Promise<void> {
    if (!params.walletSession || !params.tracker.status) {
      params.tracker.setMessage("status.not_loaded");
      return;
    }
    try {
      setRetryLoading(true);
      params.tracker.setMessage(null);
      const identity = await requireIdentity();
      const requestId = await retryFailedUnwrap(
        parseRequestIdHex(params.tracker.status.requestId),
        identity,
      );
      await queryAndStartPolling(bytesToHex(requestId));
      params.tracker.setMessage("retry.success");
    } catch (error) {
      params.tracker.setMessage(error instanceof Error ? error.message : "retry_failed");
    } finally {
      setRetryLoading(false);
    }
  }

  return {
    retryLoading,
    submitLoading,
    withdrawLoading,
    wrapActionStep,
    wrapFeeEstimate,
    wrapFeeEstimateText,
    wrapGasDetails,
    lastSubmittedWrapRequestId,
    submitUnwrap,
    submitWrap,
    retryUnwrap,
    withdraw,
    queryAndStartPolling,
  };
}

export const wrapperActionsTestHooks = {
  persistSubmittedRequest,
};
