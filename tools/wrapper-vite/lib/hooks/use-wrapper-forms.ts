// どこで: wrapper dashboard hook / 何を: form state と wrap 見積もり補助を管理 / なぜ: 画面部品と送信ロジックの責務を分離するため

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  WrapActionStep,
  WrapGasEstimateStatus,
  WrapNonceStatus,
  UnwrapFormState,
  WrapFormState,
} from "@/components/dashboard-ui/types";
import { estimateWrapGasLimit, getWrapEvmNonce } from "@/lib/canister/wrapper-client";
import { DEFAULT_ASSET_ID } from "@/lib/asset-catalog";
import { getLedgerDecimals } from "@/lib/canister/icrc2-client";
import { callerEvmAddressFromPrincipalText, principalTextToBytes } from "@/lib/principal";
import {
  deriveWrapRequestId,
  tokenAmountToBytes32,
} from "@/lib/request-id";
import { bytesToHex, hexToBytes } from "@/lib/utils";
import { parsePositiveU64, parseTokenAmount, parseU64 } from "@/lib/wrap-input";
import { buildWrapEstimateCallObject } from "@/lib/wrap-estimate";

type WrapNonceRefreshArgs = {
  walletPrincipalText: string | null;
  wrapCanisterId: string;
  readWrapNonce: typeof getWrapEvmNonce;
  onIdle: () => void;
  onLoading: () => void;
  onReady: (nonce: bigint) => void;
  onError: (message: string) => void;
  isCurrent: () => boolean;
};

export async function refreshWrapNonceState(args: WrapNonceRefreshArgs): Promise<void> {
  if (args.walletPrincipalText === null || args.wrapCanisterId.trim() === "") {
    if (args.isCurrent()) {
      args.onIdle();
    }
    return;
  }

  if (args.isCurrent()) {
    args.onLoading();
  }

  try {
    const nonce = await args.readWrapNonce(args.wrapCanisterId);
    if (!args.isCurrent()) {
      return;
    }
    args.onReady(nonce);
  } catch (error: unknown) {
    if (!args.isCurrent()) {
      return;
    }
    args.onError(error instanceof Error ? error.message : "wrap.nonce_failed");
  }
}

export function useWrapperForms(params: {
  walletPrincipalText: string | null;
  wrapCanisterId: string;
  evmWrapFactory: string;
  wrapActionStep: WrapActionStep;
}) {
  const [unwrapForm, setUnwrapForm] = useState<UnwrapFormState>({
    assetId: DEFAULT_ASSET_ID,
    amount: "",
    recipient: "",
  });
  const [wrapForm, setWrapForm] = useState<WrapFormState>({
    assetId: DEFAULT_ASSET_ID,
    amount: "",
    evmRecipient: "",
    evmNonce: "",
    gasLimit: "",
  });
  const [wrapGasEstimateStatus, setWrapGasEstimateStatus] = useState<WrapGasEstimateStatus>("idle");
  const [wrapGasEstimateError, setWrapGasEstimateError] = useState<string | null>(null);
  const [wrapNonceStatus, setWrapNonceStatus] = useState<WrapNonceStatus>("idle");
  const [wrapNonceError, setWrapNonceError] = useState<string | null>(null);
  const [unwrapAssetDecimals, setUnwrapAssetDecimals] = useState<number | null>(null);
  const [unwrapAssetDecimalsError, setUnwrapAssetDecimalsError] = useState<string | null>(null);
  const [wrapAssetDecimals, setWrapAssetDecimals] = useState<number | null>(null);
  const [wrapAssetDecimalsError, setWrapAssetDecimalsError] = useState<string | null>(null);
  const wrapNonceRequestSeq = useRef(0);

  const refreshWrapNonce = useCallback(async (): Promise<void> => {
    const requestSeq = wrapNonceRequestSeq.current + 1;
    wrapNonceRequestSeq.current = requestSeq;
    await refreshWrapNonceState({
      walletPrincipalText: params.walletPrincipalText,
      wrapCanisterId: params.wrapCanisterId,
      readWrapNonce: getWrapEvmNonce,
      onIdle: () => {
        setWrapNonceStatus("idle");
        setWrapNonceError(null);
        setWrapForm((current) => (current.evmNonce === "" ? current : { ...current, evmNonce: "" }));
      },
      onLoading: () => {
        setWrapNonceStatus("loading");
        setWrapNonceError(null);
      },
      onReady: (nonce) => {
        setWrapNonceStatus("ready");
        setWrapNonceError(null);
        setWrapForm((current) => (
          current.evmNonce === nonce.toString()
            ? current
            : { ...current, evmNonce: nonce.toString() }
        ));
      },
      onError: (message) => {
        setWrapNonceStatus("error");
        setWrapNonceError(message);
        setWrapForm((current) => (current.evmNonce === "" ? current : { ...current, evmNonce: "" }));
      },
      isCurrent: () => wrapNonceRequestSeq.current === requestSeq,
    });
  }, [params.walletPrincipalText, params.wrapCanisterId]);

  useEffect(() => {
    const principalText = params.walletPrincipalText;
    if (!principalText) {
      return;
    }
    setUnwrapForm((current) =>
      current.recipient.trim() === ""
        ? { ...current, recipient: principalText }
        : current,
    );
    setWrapForm((current) => {
      if (current.evmRecipient.trim() !== "") {
        return current;
      }
      try {
        return {
          ...current,
          evmRecipient: bytesToHex(callerEvmAddressFromPrincipalText(principalText)),
        };
      } catch {
        return current;
      }
    });
  }, [params.walletPrincipalText]);

  useEffect(() => {
    void refreshWrapNonce();
  }, [refreshWrapNonce]);

  useEffect(() => {
    if (
      params.wrapActionStep !== "done"
      || params.walletPrincipalText === null
      || params.wrapCanisterId.trim() === ""
    ) {
      return;
    }
    let attempts = 0;
    const timer = window.setInterval(() => {
      attempts += 1;
      void refreshWrapNonce();
      if (attempts >= 6) {
        window.clearInterval(timer);
      }
    }, 10_000);
    return () => {
      window.clearInterval(timer);
    };
  }, [
    params.walletPrincipalText,
    params.wrapActionStep,
    params.wrapCanisterId,
    refreshWrapNonce,
  ]);

  useEffect(() => {
    const assetId = unwrapForm.assetId.trim();
    if (assetId === "") {
      setUnwrapAssetDecimals(null);
      setUnwrapAssetDecimalsError(null);
      return;
    }

    let cancelled = false;
    setUnwrapAssetDecimals(null);
    setUnwrapAssetDecimalsError(null);
    void getLedgerDecimals(assetId)
      .then((decimals) => {
        if (cancelled) {
          return;
        }
        setUnwrapAssetDecimals(decimals);
        setUnwrapAssetDecimalsError(null);
      })
      .catch((error: unknown) => {
        if (cancelled) {
          return;
        }
        setUnwrapAssetDecimals(null);
        setUnwrapAssetDecimalsError(
          error instanceof Error ? error.message : "wrap.asset_metadata_failed:query_failed",
        );
      });

    return () => {
      cancelled = true;
    };
  }, [unwrapForm.assetId]);

  useEffect(() => {
    const assetId = wrapForm.assetId.trim();
    if (assetId === "") {
      setWrapAssetDecimals(null);
      setWrapAssetDecimalsError(null);
      return;
    }

    let cancelled = false;
    setWrapAssetDecimals(null);
    setWrapAssetDecimalsError(null);
    void getLedgerDecimals(assetId)
      .then((decimals) => {
        if (cancelled) {
          return;
        }
        setWrapAssetDecimals(decimals);
        setWrapAssetDecimalsError(null);
      })
      .catch((error: unknown) => {
        if (cancelled) {
          return;
        }
        setWrapAssetDecimals(null);
        setWrapAssetDecimalsError(
          error instanceof Error ? error.message : "wrap.asset_metadata_failed:query_failed",
        );
      });

    return () => {
      cancelled = true;
    };
  }, [wrapForm.assetId]);

  useEffect(() => {
    const assetId = wrapForm.assetId.trim();
    const amount = wrapForm.amount.trim();
    const evmRecipient = wrapForm.evmRecipient.trim();
    const evmNonce = wrapForm.evmNonce.trim();

    if (
      wrapNonceStatus !== "ready" ||
      wrapAssetDecimals === null ||
      assetId === "" ||
      amount === "" ||
      evmRecipient === "" ||
      evmNonce === ""
    ) {
      setWrapGasEstimateStatus(wrapAssetDecimalsError === null ? "idle" : "error");
      setWrapGasEstimateError(wrapAssetDecimalsError);
      setWrapForm((current) => (current.gasLimit === "" ? current : { ...current, gasLimit: "" }));
      return;
    }

    try {
      parseU64(evmNonce, "validation.evm_nonce.invalid");
      parseTokenAmount(amount, wrapAssetDecimals, "validation.amount.invalid");
      buildWrapEstimateCallObject({
        wrapCanisterId: params.wrapCanisterId,
        evmWrapFactory: params.evmWrapFactory,
        assetId,
        tokenDecimals: wrapAssetDecimals,
        amount,
        evmRecipient,
      });
    } catch {
      setWrapGasEstimateStatus("idle");
      setWrapGasEstimateError(null);
      setWrapForm((current) => (current.gasLimit === "" ? current : { ...current, gasLimit: "" }));
      return;
    }

    let cancelled = false;
    setWrapGasEstimateStatus("estimating");
    setWrapGasEstimateError(null);
    void estimateWrapGasLimit({
      wrapCanisterId: params.wrapCanisterId,
      evmWrapFactory: params.evmWrapFactory,
      assetId,
      tokenDecimals: wrapAssetDecimals,
      amount,
      evmRecipient,
    }).then((gasLimit) => {
      if (cancelled) {
        return;
      }
      setWrapGasEstimateStatus("ready");
      setWrapGasEstimateError(null);
      setWrapForm((current) => (
        current.gasLimit === gasLimit.toString()
          ? current
          : { ...current, gasLimit: gasLimit.toString() }
      ));
    }).catch((error: unknown) => {
      if (cancelled) {
        return;
      }
      setWrapGasEstimateStatus("error");
      setWrapGasEstimateError(
        error instanceof Error ? error.message : "wrap.gas_estimate_failed",
      );
      setWrapForm((current) => (current.gasLimit === "" ? current : { ...current, gasLimit: "" }));
    });

    return () => {
      cancelled = true;
    };
  }, [
    params.evmWrapFactory,
    params.wrapCanisterId,
    wrapForm.assetId,
    wrapForm.amount,
    wrapForm.evmNonce,
    wrapForm.evmRecipient,
    wrapAssetDecimals,
    wrapAssetDecimalsError,
    wrapNonceStatus,
  ]);

  const wrapPreviewRequestId = useMemo(() => {
    if (!params.walletPrincipalText) {
      return null;
    }
    if (wrapGasEstimateStatus !== "ready" || wrapAssetDecimals === null) {
      return null;
    }
    try {
      return bytesToHex(
        deriveWrapRequestId({
          fromOwner: principalTextToBytes(params.walletPrincipalText),
          assetId: principalTextToBytes(wrapForm.assetId.trim()),
          amount: tokenAmountToBytes32(wrapForm.amount.trim(), wrapAssetDecimals),
          evmRecipient: hexToBytes(wrapForm.evmRecipient.trim()),
          evmNonce: parseU64(wrapForm.evmNonce, "validation.evm_nonce.invalid"),
          gasLimit: parsePositiveU64(wrapForm.gasLimit, "validation.gas_limit.invalid"),
        }),
      );
    } catch {
      return null;
    }
  }, [params.walletPrincipalText, wrapAssetDecimals, wrapForm, wrapGasEstimateStatus]);

  function resetUnwrapNonceDeadline(): void {
    setUnwrapForm((current) => current);
  }

  return {
    unwrapForm,
    setUnwrapForm,
    wrapForm,
    setWrapForm,
    wrapPreviewRequestId,
    unwrapAssetDecimals,
    unwrapAssetDecimalsError,
    wrapAssetDecimals,
    wrapAssetDecimalsError,
    wrapGasEstimateStatus,
    wrapGasEstimateError,
    wrapNonceStatus,
    wrapNonceError,
    resetUnwrapNonceDeadline,
    refreshWrapNonce,
  };
}
