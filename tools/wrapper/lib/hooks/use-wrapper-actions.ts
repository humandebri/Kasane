// どこで: wrapper dashboard hook / 何を: unwrap/wrap/withdraw送信処理を提供 / なぜ: 画面コンポーネントから副作用ロジックを分離するため

import { useEffect, useState } from "react";
import type {
  HistoryEntry,
  UnwrapFormState,
  WrapActionStep,
  WrapFormState,
} from "@/components/dashboard-ui/types";
import { approveLedgerSpend, getLedgerAllowance } from "@/lib/canister/icrc2-client";
import { approveWrappedTokenIfNeeded } from "@/lib/canister/erc20-client";
import {
  estimateUnwrapGasLimit,
  getExpectedNonce,
  getGasPriceWei,
  submitIcTx,
} from "@/lib/canister/wrapper-client";
import {
  getFeePolicy,
  retryFailedUnwrap,
  submitWrapRequest,
  withdrawFailedWrap,
} from "@/lib/canister/wrap-client";
import type { loadConfig } from "@/lib/config";
import { callerEvmAddressFromPrincipalText, principalTextToBytes } from "@/lib/principal";
import {
  decimalToBytes32,
  deriveWrapRequestId,
  toSubmitIcTxData,
  WRAP_PRECOMPILE_ADDRESS,
} from "@/lib/request-id";
import { bytesToHex, hexToBytes, parseRequestIdHex } from "@/lib/utils";
import {
  computeRequiredAllowances,
  computeWrapFeeQuote,
  formatE8sToIcpText,
} from "@/lib/wrap-flow";
import { parsePositiveBigInt, parsePositiveU64, parseU64 } from "@/lib/wrap-input";
import type { WalletSession } from "@/lib/wallet/types";

type AppConfig = ReturnType<typeof loadConfig>;

type StatusTrackerState = {
  status: { requestId: string } | null;
  setMessage: (value: string | null) => void;
  refreshStatus: (requestIdHex: string, background?: boolean) => Promise<boolean>;
  setAutoPolling: (value: boolean) => void;
};

type WrapperFormsState = {
  unwrapForm: UnwrapFormState;
  wrapForm: WrapFormState;
  wrapGasEstimateStatus: "idle" | "estimating" | "ready" | "error";
  wrapGasEstimateError: string | null;
  wrapNonceStatus: "idle" | "loading" | "ready" | "error";
  wrapNonceError: string | null;
  resetUnwrapNonceDeadline: () => void;
};

export function useWrapperActions(params: {
  cfg: AppConfig | null;
  configError: string | null;
  walletSession: WalletSession | null;
  forms: WrapperFormsState;
  tracker: StatusTrackerState;
  onRequestSubmitted: (entry: HistoryEntry) => void;
  onRequestIdInput: (requestId: string) => void;
}) {
  const [submitLoading, setSubmitLoading] = useState(false);
  const [retryLoading, setRetryLoading] = useState(false);
  const [withdrawLoading, setWithdrawLoading] = useState(false);
  const [wrapActionStep, setWrapActionStep] = useState<WrapActionStep>("idle");
  const [wrapFeeEstimateText, setWrapFeeEstimateText] = useState<string | null>(null);

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

  async function queryAndStartPolling(trackingIdHex: string): Promise<void> {
    const ok = await params.tracker.refreshStatus(trackingIdHex);
    if (ok) {
      params.tracker.setAutoPolling(true);
    }
  }

  async function submitUnwrap(): Promise<void> {
    const ready = requireReady();
    if (!ready || !params.walletSession) {
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
      const amount = parsePositiveBigInt(
        params.forms.unwrapForm.amount,
        "validation.amount.invalid",
      );
      await approveWrappedTokenIfNeeded({
        assetId: params.forms.unwrapForm.assetId.trim(),
        amount,
        principalText: ready.principalText,
        identity: params.walletSession.identity,
      });
      const callerEvmAddress = callerEvmAddressFromPrincipalText(ready.principalText);
      const txData = toSubmitIcTxData({
        assetId: params.forms.unwrapForm.assetId.trim(),
        amount,
        recipient: params.forms.unwrapForm.recipient.trim(),
      });
      const nonce = await getExpectedNonce(callerEvmAddress);
      const gasLimit = await estimateUnwrapGasLimit({
        callerEvmAddress,
        nonce,
        data: txData,
      });
      const txId = await submitIcTx({
        to: WRAP_PRECOMPILE_ADDRESS,
        data: txData,
        nonce,
        gasLimit,
        identity: params.walletSession.identity,
      });
      const txIdHex = bytesToHex(txId);
      params.onRequestIdInput(txIdHex);
      params.onRequestSubmitted({
        requestId: txIdHex,
        kind: "unwrap",
        submittedAt: new Date().toISOString(),
      });
      await queryAndStartPolling(txIdHex);
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
    if (!ready || !params.walletSession) {
      return;
    }
    try {
      setSubmitLoading(true);
      params.tracker.setMessage(null);
      setWrapActionStep("quoting");
      if (params.forms.wrapForm.assetId.trim() === "") {
        throw new Error("validation.asset_id_required");
      }
      if (params.forms.wrapForm.evmRecipient.trim() === "") {
        throw new Error("validation.evm_recipient_required");
      }
      const amount = parsePositiveBigInt(
        params.forms.wrapForm.amount,
        "validation.amount.invalid",
      );
      if (params.forms.wrapNonceStatus !== "ready") {
        throw new Error(params.forms.wrapNonceError ?? "wrap.nonce_failed");
      }
      const evmNonce = parseU64(
        params.forms.wrapForm.evmNonce,
        "validation.evm_nonce.invalid",
      );
      if (params.forms.wrapGasEstimateStatus !== "ready") {
        throw new Error(params.forms.wrapGasEstimateError ?? "wrap.gas_estimate_failed");
      }
      const gasLimit = parsePositiveU64(
        params.forms.wrapForm.gasLimit,
        "validation.gas_limit.invalid",
      );
      const [feePolicy, gasPriceWei] = await Promise.all([
        getFeePolicy(),
        getGasPriceWei(),
      ]);
      const quote = computeWrapFeeQuote({
        gasPriceWei,
        gasLimit,
        cycleFeeE8s: feePolicy.cycleFeeE8s,
        gasPriceBufferBps: BigInt(feePolicy.gasPriceBufferBps),
      });
      setWrapFeeEstimateText(
        `estimated fee: ${formatE8sToIcpText(quote.totalFeeE8s)} ICP (${quote.totalFeeE8s.toString()} e8s)`,
      );

      setWrapActionStep("checking_allowance");
      const required = computeRequiredAllowances({
        assetLedgerCanister: params.forms.wrapForm.assetId.trim(),
        feeLedgerCanister: feePolicy.feeLedgerCanister,
        amount,
        totalFeeE8s: quote.totalFeeE8s,
      });
      const ownerPrincipalText = ready.principalText;
      const spenderCanisterId = ready.cfg.wrapCanisterId.trim();
      const assetAllowance = await getLedgerAllowance({
        ledgerCanisterId: params.forms.wrapForm.assetId.trim(),
        ownerPrincipalText,
        spenderCanisterId,
      });
      if (assetAllowance < required.requiredAssetAllowance) {
        setWrapActionStep("approving_asset");
        await approveLedgerSpend({
          ledgerCanisterId: params.forms.wrapForm.assetId.trim(),
          spenderCanisterId,
          amount: required.requiredAssetAllowance,
          identity: params.walletSession.identity,
        });
      }
      if (required.requiredFeeAllowance > 0n) {
        const feeAllowance = await getLedgerAllowance({
          ledgerCanisterId: feePolicy.feeLedgerCanister,
          ownerPrincipalText,
          spenderCanisterId,
        });
        if (feeAllowance < required.requiredFeeAllowance) {
          setWrapActionStep("approving_fee");
          await approveLedgerSpend({
            ledgerCanisterId: feePolicy.feeLedgerCanister,
            spenderCanisterId,
            amount: required.requiredFeeAllowance,
            identity: params.walletSession.identity,
          });
        }
      }

      setWrapActionStep("submitting");
      const requestId = deriveWrapRequestId({
        fromOwner: principalTextToBytes(params.walletSession.principalText),
        assetId: principalTextToBytes(params.forms.wrapForm.assetId.trim()),
        amount: decimalToBytes32(params.forms.wrapForm.amount.trim()),
        evmRecipient: hexToBytes(params.forms.wrapForm.evmRecipient.trim()),
        evmNonce,
        gasLimit,
      });
      await submitWrapRequest(
        {
          requestId,
          assetId: principalTextToBytes(params.forms.wrapForm.assetId.trim()),
          amount: decimalToBytes32(params.forms.wrapForm.amount.trim()),
          evmRecipient: hexToBytes(params.forms.wrapForm.evmRecipient.trim()),
          evmNonce,
          gasLimit,
        },
        params.walletSession.identity,
      );
      const requestIdHex = bytesToHex(requestId);
      params.onRequestIdInput(requestIdHex);
      params.onRequestSubmitted({
        requestId: requestIdHex,
        kind: "wrap",
        submittedAt: new Date().toISOString(),
      });
      await queryAndStartPolling(requestIdHex);
      setWrapActionStep("done");
      params.tracker.setMessage(`wrap.submit.success fee=${quote.totalFeeE8s.toString()}e8s`);
    } catch (error) {
      setWrapActionStep("error");
      params.tracker.setMessage(
        error instanceof Error ? error.message : "wrap_submit_failed",
      );
    } finally {
      setSubmitLoading(false);
    }
  }

  useEffect(() => {
    if (params.forms.wrapGasEstimateStatus !== "ready") {
      setWrapFeeEstimateText(null);
      return;
    }
    let cancelled = false;
    void Promise.all([getFeePolicy(), getGasPriceWei()])
      .then(([feePolicy, gasPriceWei]) => {
        if (cancelled) {
          return;
        }
        const gasLimit = parsePositiveU64(
          params.forms.wrapForm.gasLimit,
          "validation.gas_limit.invalid",
        );
        const quote = computeWrapFeeQuote({
          gasPriceWei,
          gasLimit,
          cycleFeeE8s: feePolicy.cycleFeeE8s,
          gasPriceBufferBps: BigInt(feePolicy.gasPriceBufferBps),
        });
        setWrapFeeEstimateText(
          `estimated fee: ${formatE8sToIcpText(quote.totalFeeE8s)} ICP (${quote.totalFeeE8s.toString()} e8s)`,
        );
      })
      .catch(() => {
        if (!cancelled) {
          setWrapFeeEstimateText(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [params.forms.wrapForm.gasLimit, params.forms.wrapGasEstimateStatus]);

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
      await withdrawFailedWrap(
        parseRequestIdHex(params.tracker.status.requestId),
        params.walletSession.identity,
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
      const requestId = await retryFailedUnwrap(
        parseRequestIdHex(params.tracker.status.requestId),
        params.walletSession.identity,
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
    wrapFeeEstimateText,
    submitUnwrap,
    submitWrap,
    retryUnwrap,
    withdraw,
    queryAndStartPolling,
  };
}
