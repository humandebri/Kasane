// どこで: wrapper dashboard hook / 何を: Oisy wrap系と MetaMask unwrap系の送信処理を提供 / なぜ: signer と EVM sender の認証境界を UI から切り離すため

import { useCallback, useEffect, useState } from "react";
import type {
  HistoryEntry,
  UnwrapFormState,
  WrapActionStep,
  WrapFormState,
} from "@/components/dashboard-ui/types";
import { approveLedgerSpend, getLedgerAllowance } from "@/lib/canister/icrc2-client";
import {
  getMaxPriorityFeePerGasWei,
  getUnwrapRequestIdsByEthTxHash,
} from "@/lib/canister/wrapper-client";
import {
  getUnwrapRequirements,
  quoteNativeDeposit,
  quoteWrapRequest,
  retryFailedUnwrap,
  submitNativeDeposit,
  submitWrapRequest,
  withdrawFailedWrap,
} from "@/lib/canister/wrap-client";
import type { loadConfig } from "@/lib/config";
import type { AuthenticatedCaller } from "@/lib/canister/authenticated-caller";
import { encodeApproveCall } from "@/lib/erc20";
import {
  estimateMetaMaskUnwrapTransaction,
  getKasaneTransactionStatus,
  sendMetaMaskTransaction,
} from "@/lib/kasane-rpc";
import {
  ensureMetaMaskChain,
  getMetaMaskProvider,
  normalizeMetaMaskAddress,
} from "@/lib/wallet/metamask";
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
import type { MetaMaskSession, WalletSession } from "@/lib/wallet/types";

type OisyCapabilityState = {
  ledgerApproveSupported: boolean;
  wrapCanisterSupported: boolean;
  gatewaySupported: boolean;
};

type AppConfig = ReturnType<typeof loadConfig>;

type StatusTrackerState = {
  status: { requestId: string } | null;
  setStatus: (value: {
    kind: "request";
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

async function sleep(ms: number): Promise<void> {
  await new Promise((resolve) => {
    globalThis.setTimeout(resolve, ms);
  });
}

function createNativeDepositId(): Uint8Array {
  const crypto = globalThis.crypto;
  if (!crypto) {
    throw new Error("native_deposit.crypto_unavailable");
  }
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return bytes;
}

async function waitForTransactionFinal(args: {
  rpcUrl: string;
  explorerBaseUrl: string | null;
  transactionHash: string;
}): Promise<void> {
  for (let attempt = 0; attempt < 30; attempt += 1) {
    const status = await getKasaneTransactionStatus({
      rpcUrl: args.rpcUrl,
      explorerBaseUrl: args.explorerBaseUrl,
      transactionHash: args.transactionHash,
    });
    if (status.transactionStatus === "Succeeded") {
      return;
    }
    if (status.transactionStatus === "Failed") {
      throw new Error(status.errorCode ?? "kasane.tx_failed");
    }
    await sleep(2_000);
  }
  throw new Error("kasane.tx_timeout");
}

async function finishSubmittedUnwrapRequest(args: {
  requestIdHex: string;
  onRequestIdInput: (requestId: string) => void;
  onRequestSubmitted: (entry: HistoryEntry) => Promise<void> | void;
  startPollingSubmittedRequest: (requestIdHex: string) => Promise<void>;
  setMessage: (value: string | null) => void;
  resetUnwrapNonceDeadline: () => void;
}): Promise<void> {
  args.onRequestIdInput(args.requestIdHex);
  await args.startPollingSubmittedRequest(args.requestIdHex);
  persistSubmittedRequest(args.onRequestSubmitted, {
    requestId: args.requestIdHex,
    kind: "unwrap",
    submittedAt: new Date().toISOString(),
  });
  args.setMessage("submit.success");
  args.resetUnwrapNonceDeadline();
}

export function useWrapperActions(params: {
  cfg: AppConfig | null;
  configError: string | null;
  oisySession: WalletSession | null;
  oisyCapabilities: OisyCapabilityState;
  metaMaskSession: MetaMaskSession | null;
  getCaller: () => Promise<AuthenticatedCaller | null>;
  forms: WrapperFormsState;
  tracker: StatusTrackerState;
  onRequestSubmitted: (entry: HistoryEntry) => Promise<void> | void;
  onRequestIdInput: (requestId: string) => void;
  onMetaMaskTransactionSubmitted: (transactionHash: string) => void;
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

  function requireOisyReady(): { cfg: AppConfig; principalText: string } | null {
    if (!params.cfg) {
      params.tracker.setMessage(params.configError ?? "config.invalid");
      return null;
    }
    if (!params.oisySession) {
      params.tracker.setMessage("wallet.not_connected");
      return null;
    }
    return { cfg: params.cfg, principalText: params.oisySession.principalText };
  }

  function requireWrapCanisterSupport(): boolean {
    if (!params.oisyCapabilities.wrapCanisterSupported) {
      params.tracker.setMessage("wallet.oisy_wrap_canister_unsupported");
      return false;
    }
    return true;
  }

  async function requireCaller(): Promise<AuthenticatedCaller> {
    const caller = await params.getCaller();
    if (caller === null) {
      throw new Error("wallet.not_connected");
    }
    return caller;
  }

  const queryAndStartPolling = useCallback(async (trackingIdHex: string): Promise<void> => {
    const ok = await params.tracker.refreshStatus(trackingIdHex);
    if (ok) {
      params.tracker.setAutoPolling(true);
    }
  }, [params.tracker]);

  const startPollingSubmittedRequest = useCallback(async (requestIdHex: string): Promise<void> => {
    params.tracker.setStatus({
      kind: "request",
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

  async function resolveUnwrapRequestIdHexByEthTxHash(ethTxHash: Uint8Array): Promise<string> {
    for (let attempt = 0; attempt < 20; attempt += 1) {
      const requestIds = await getUnwrapRequestIdsByEthTxHash(ethTxHash);
      if (requestIds.length === 1) {
        const requestId = requestIds[0];
        if (requestId !== undefined) {
          return bytesToHex(requestId);
        }
      }
      if (requestIds.length > 1) {
        throw new Error("status.unwrap_tx.multiple_request_ids");
      }
      await sleep(500);
    }
    throw new Error("status.unwrap_tx.request_id_unresolved");
  }

  async function submitMetaMaskUnwrap(): Promise<void> {
    if (!params.cfg) {
      throw new Error(params.configError ?? "config.invalid");
    }
    if (params.metaMaskSession === null) {
      throw new Error("wallet.metamask_not_connected");
    }
    if (params.forms.unwrapAssetDecimals === null) {
      throw new Error(params.forms.unwrapAssetDecimalsError ?? "wrap.asset_metadata_failed");
    }
    const provider = getMetaMaskProvider();
    if (provider === null) {
      throw new Error("wallet.metamask_missing");
    }
    const fromAddress = normalizeMetaMaskAddress(params.metaMaskSession.accountAddress);
    const amount = parseTokenAmount(
      params.forms.unwrapForm.amount,
      params.forms.unwrapAssetDecimals,
      "validation.amount.invalid",
    );
    const callerEvmAddress = hexToBytes(fromAddress);
    const requirements = await getUnwrapRequirements({
      assetId: params.forms.unwrapForm.assetId.trim(),
      amountE8s: amount,
      callerEvmAddress,
    });
    if (requirements.wrappedTokenAddress === null || requirements.readiness === "TokenNotDeployed") {
      throw new Error("unwrap.token_not_deployed");
    }
    if (requirements.readiness === "InsufficientBalance") {
      throw new Error("erc20.insufficient_balance");
    }
    const chainConfig = {
      chainId: params.cfg.kasaneChainId,
      chainName: params.cfg.kasaneChainName,
      rpcUrl: params.cfg.kasaneRpcUrl,
      nativeCurrencySymbol: params.cfg.kasaneNativeCurrencySymbol,
      blockExplorerUrl: params.cfg.kasaneBlockExplorerUrl,
    };
    await ensureMetaMaskChain(provider, chainConfig);
    if (requirements.approveRequired || requirements.readiness === "InsufficientAllowance") {
      const approveData = bytesToHex(encodeApproveCall(requirements.factoryAddress, amount));
      const approveTarget = bytesToHex(requirements.wrappedTokenAddress);
      const approveEstimate = await estimateMetaMaskUnwrapTransaction({
        rpcUrl: params.cfg.kasaneRpcUrl,
        from: fromAddress,
        to: approveTarget,
        data: approveData,
      });
      const approveHash = await sendMetaMaskTransaction({
        provider,
        chainConfig,
        from: fromAddress,
        to: approveTarget,
        data: approveData,
        nonce: approveEstimate.nonce,
        gas: approveEstimate.gas,
        maxFeePerGas: approveEstimate.maxFeePerGas,
        maxPriorityFeePerGas: approveEstimate.maxPriorityFeePerGas,
      });
      await waitForTransactionFinal({
        rpcUrl: params.cfg.kasaneRpcUrl,
        explorerBaseUrl: params.cfg.kasaneBlockExplorerUrl,
        transactionHash: approveHash,
      });
    }
    const unwrapData = bytesToHex(toSubmitIcTxData({
      assetId: params.forms.unwrapForm.assetId.trim(),
      amount,
      recipient: params.forms.unwrapForm.recipient.trim(),
    }));
    const unwrapEstimate = await estimateMetaMaskUnwrapTransaction({
      rpcUrl: params.cfg.kasaneRpcUrl,
      from: fromAddress,
      to: bytesToHex(WRAP_PRECOMPILE_ADDRESS),
      data: unwrapData,
    });
    const transactionHash = await sendMetaMaskTransaction({
      provider,
      chainConfig,
      from: fromAddress,
      to: bytesToHex(WRAP_PRECOMPILE_ADDRESS),
      data: unwrapData,
      nonce: unwrapEstimate.nonce,
      gas: unwrapEstimate.gas,
      maxFeePerGas: unwrapEstimate.maxFeePerGas,
      maxPriorityFeePerGas: unwrapEstimate.maxPriorityFeePerGas,
    });
    params.onMetaMaskTransactionSubmitted(transactionHash);
    await waitForTransactionFinal({
      rpcUrl: params.cfg.kasaneRpcUrl,
      explorerBaseUrl: params.cfg.kasaneBlockExplorerUrl,
      transactionHash,
    });
    const requestIdHex = await resolveUnwrapRequestIdHexByEthTxHash(hexToBytes(transactionHash));
    await finishSubmittedUnwrapRequest({
      requestIdHex,
      onRequestIdInput: params.onRequestIdInput,
      onRequestSubmitted: params.onRequestSubmitted,
      startPollingSubmittedRequest,
      setMessage: params.tracker.setMessage,
      resetUnwrapNonceDeadline: params.forms.resetUnwrapNonceDeadline,
    });
  }

  async function submitUnwrap(): Promise<void> {
    try {
      setSubmitLoading(true);
      params.tracker.setMessage(null);
      if (params.forms.unwrapForm.assetId.trim() === "") {
        throw new Error("validation.asset_id_required");
      }
      if (params.forms.unwrapForm.recipient.trim() === "") {
        throw new Error("validation.recipient_required");
      }
      await submitMetaMaskUnwrap();
    } catch (error) {
      params.tracker.setMessage(error instanceof Error ? error.message : "submit_failed");
    } finally {
      setSubmitLoading(false);
    }
  }

  async function submitWrap(): Promise<void> {
    const ready = requireOisyReady();
    if (!ready) {
      return;
    }
    if (!requireWrapCanisterSupport()) {
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
      const assetId = params.forms.wrapForm.assetId.trim();
      const evmRecipient = hexToBytes(params.forms.wrapForm.evmRecipient.trim());
      const nativeQuote = await quoteNativeDeposit({
        amountE8s: amount,
        evmRecipient,
      });
      const isNativeDeposit = assetId === nativeQuote.nativeLedgerCanister;
      let quote: { chargedFeeE8s: bigint; feeLedgerCanister: string } = nativeQuote;
      let gasLimit: bigint | null = null;
      let evmNonce: bigint | null = null;
      let chargedGasPriceWei: bigint | null = null;
      if (!isNativeDeposit) {
        if (params.forms.wrapGasEstimateStatus !== "ready") {
          throw new Error(params.forms.wrapGasEstimateError ?? "wrap.gas_estimate_failed");
        }
        gasLimit = parsePositiveU64(
          params.forms.wrapForm.gasLimit,
          "validation.gas_limit.invalid",
        );
        evmNonce = parseU64(
          params.forms.wrapForm.evmNonce,
          "validation.evm_nonce.invalid",
        );
        const wrapQuote = await quoteWrapRequest({
          assetId,
          amountE8s: amount,
          evmRecipient,
          gasLimit,
        });
        quote = wrapQuote;
        chargedGasPriceWei = wrapQuote.chargedGasPriceWei;
      }
      setWrapFeeEstimate({
        chargedFeeE8s: quote.chargedFeeE8s,
        feeLedgerCanister: quote.feeLedgerCanister,
      });
      if (chargedGasPriceWei === null) {
        setWrapGasDetails(null);
      } else {
        const priorityFeeWei = await getMaxPriorityFeePerGasWei().catch(() => null);
        setWrapGasDetails(priorityFeeWei === null ? null : {
          chargedGasPriceWei,
          maxPriorityFeePerGasWei: priorityFeeWei,
        });
      }
      setWrapFeeEstimateText(
        `estimated fee: ${formatE8sToIcpText4(quote.chargedFeeE8s)} ICP`,
      );

      updateWrapActionStep("checking_allowance");
      const required = computeRequiredAllowances({
        assetLedgerCanister: assetId,
        feeLedgerCanister: quote.feeLedgerCanister,
        amount,
        totalFeeE8s: quote.chargedFeeE8s,
      });
      const ownerPrincipalText = ready.principalText;
      const spenderCanisterId = ready.cfg.wrapCanisterId.trim();
      const caller = await requireCaller();
      const assetAllowance = await getLedgerAllowance({
        ledgerCanisterId: assetId,
        ownerPrincipalText,
        spenderCanisterId,
      });
      if (assetAllowance < required.requiredAssetAllowance) {
        updateWrapActionStep("approving_asset");
          await approveLedgerSpend({
            ledgerCanisterId: assetId,
            spenderCanisterId,
            amount: required.requiredAssetAllowance,
            caller,
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
            caller,
          });
        }
      }

      updateWrapActionStep("submitting");
      const submitResult = isNativeDeposit
        ? await submitNativeDeposit({
          depositId: createNativeDepositId(),
          amountE8s: amount,
          evmRecipient,
          maxFeeE8s: quote.chargedFeeE8s,
          feeLedgerCanister: quote.feeLedgerCanister,
        }, caller)
        : await submitWrapRequest({
          assetId,
          amountE8s: amount,
          evmRecipient,
          evmNonce: evmNonce ?? 0n,
          gasLimit: gasLimit ?? 0n,
          maxFeeE8s: quote.chargedFeeE8s,
          quotedGasPriceWei: chargedGasPriceWei ?? 0n,
          feeLedgerCanister: quote.feeLedgerCanister,
        }, caller);
      const requestIdHex = bytesToHex(submitResult.requestId);
      setLastSubmittedWrapRequestId(requestIdHex);
      params.onRequestIdInput(requestIdHex);
      await startPollingSubmittedRequest(requestIdHex);
      persistSubmittedRequest(params.onRequestSubmitted, {
        requestId: requestIdHex,
        kind: "wrap",
        submittedAt: new Date().toISOString(),
      });
      if (!isNativeDeposit) {
        await params.forms.refreshWrapNonce().catch(() => undefined);
      }
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
      evmRecipientBytes = hexToBytes(evmRecipient);
    } catch {
      setWrapFeeEstimate(null);
      setWrapGasDetails(null);
      setWrapFeeEstimateText(null);
      return;
    }
    void quoteNativeDeposit({
      amountE8s,
      evmRecipient: evmRecipientBytes,
    })
      .then(async (nativeQuote) => {
        if (cancelled) {
          return;
        }
        if (assetId === nativeQuote.nativeLedgerCanister) {
          setWrapFeeEstimate({
            chargedFeeE8s: nativeQuote.chargedFeeE8s,
            feeLedgerCanister: nativeQuote.feeLedgerCanister,
          });
          setWrapGasDetails(null);
          setWrapFeeEstimateText(
            `estimated fee: ${formatE8sToIcpText4(nativeQuote.chargedFeeE8s)} ICP`,
          );
          return;
        }
        if (params.forms.wrapGasEstimateStatus !== "ready") {
          setWrapFeeEstimate(null);
          setWrapGasDetails(null);
          setWrapFeeEstimateText(null);
          return;
        }
        const gasLimit = parsePositiveU64(
          params.forms.wrapForm.gasLimit,
          "validation.gas_limit.invalid",
        );
        const [quote, priorityFeeWei] = await Promise.all([
          quoteWrapRequest({
            assetId,
            amountE8s,
            evmRecipient: evmRecipientBytes,
            gasLimit,
          }),
          getMaxPriorityFeePerGasWei(),
        ]);
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
    if (!params.oisySession || !params.tracker.status) {
      params.tracker.setMessage("status.not_loaded");
      return;
    }
    if (!params.cfg) {
      params.tracker.setMessage(params.configError ?? "config.invalid");
      return;
    }
    if (!requireWrapCanisterSupport()) {
      return;
    }
    try {
      setWithdrawLoading(true);
      params.tracker.setMessage(null);
      const caller = await requireCaller();
      await withdrawFailedWrap(
        parseRequestIdHex(params.tracker.status.requestId),
        caller,
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
    if (!params.oisySession || !params.tracker.status) {
      params.tracker.setMessage("status.not_loaded");
      return;
    }
    if (!requireWrapCanisterSupport()) {
      return;
    }
    try {
      setRetryLoading(true);
      params.tracker.setMessage(null);
      const caller = await requireCaller();
      const requestId = await retryFailedUnwrap(
        parseRequestIdHex(params.tracker.status.requestId),
        caller,
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
  finishSubmittedUnwrapRequest,
  persistSubmittedRequest,
};
