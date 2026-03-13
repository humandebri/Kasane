// どこで: wrapper dashboard hook / 何を: form state と wrap 見積もり補助を管理 / なぜ: 画面部品と送信ロジックの責務を分離するため

import { useEffect, useMemo, useState } from "react";
import type {
  WrapGasEstimateStatus,
  WrapNonceStatus,
  UnwrapFormState,
  WrapFormState,
} from "@/components/dashboard-ui/types";
import { estimateWrapGasLimit, getWrapEvmNonce } from "@/lib/canister/wrapper-client";
import { getLedgerDecimals } from "@/lib/canister/icrc2-client";
import { callerEvmAddressFromPrincipalText, principalTextToBytes } from "@/lib/principal";
import {
  decimalToBytes32,
  deriveWrapRequestId,
} from "@/lib/request-id";
import { bytesToHex, hexToBytes } from "@/lib/utils";
import { parsePositiveU64, parseU64 } from "@/lib/wrap-input";
import { buildWrapEstimateCallObject } from "@/lib/wrap-estimate";

export function useWrapperForms(params: {
  walletPrincipalText: string | null;
  wrapCanisterId: string;
  evmWrapFactory: string;
}) {
  const [unwrapForm, setUnwrapForm] = useState<UnwrapFormState>({
    assetId: "",
    amount: "",
    recipient: "",
  });
  const [wrapForm, setWrapForm] = useState<WrapFormState>({
    assetId: "",
    amount: "",
    evmRecipient: "",
    evmNonce: "",
    gasLimit: "",
  });
  const [wrapGasEstimateStatus, setWrapGasEstimateStatus] = useState<WrapGasEstimateStatus>("idle");
  const [wrapGasEstimateError, setWrapGasEstimateError] = useState<string | null>(null);
  const [wrapNonceStatus, setWrapNonceStatus] = useState<WrapNonceStatus>("idle");
  const [wrapNonceError, setWrapNonceError] = useState<string | null>(null);
  const [wrapAssetDecimals, setWrapAssetDecimals] = useState<number | null>(null);
  const [wrapAssetDecimalsError, setWrapAssetDecimalsError] = useState<string | null>(null);

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
    if (params.walletPrincipalText === null || params.wrapCanisterId.trim() === "") {
      setWrapNonceStatus("idle");
      setWrapNonceError(null);
      setWrapForm((current) => (current.evmNonce === "" ? current : { ...current, evmNonce: "" }));
      return;
    }

    let cancelled = false;
    setWrapNonceStatus("loading");
    setWrapNonceError(null);
    void getWrapEvmNonce(params.wrapCanisterId)
      .then((nonce) => {
        if (cancelled) {
          return;
        }
        setWrapNonceStatus("ready");
        setWrapNonceError(null);
        setWrapForm((current) => (
          current.evmNonce === nonce.toString()
            ? current
            : { ...current, evmNonce: nonce.toString() }
        ));
      })
      .catch((error: unknown) => {
        if (cancelled) {
          return;
        }
        setWrapNonceStatus("error");
        setWrapNonceError(error instanceof Error ? error.message : "wrap.nonce_failed");
        setWrapForm((current) => (current.evmNonce === "" ? current : { ...current, evmNonce: "" }));
      });

    return () => {
      cancelled = true;
    };
  }, [params.walletPrincipalText, params.wrapCanisterId]);

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
    if (wrapGasEstimateStatus !== "ready") {
      return null;
    }
    try {
      return bytesToHex(
        deriveWrapRequestId({
          fromOwner: principalTextToBytes(params.walletPrincipalText),
          assetId: principalTextToBytes(wrapForm.assetId.trim()),
          amount: decimalToBytes32(wrapForm.amount.trim()),
          evmRecipient: hexToBytes(wrapForm.evmRecipient.trim()),
          gasLimit: parsePositiveU64(wrapForm.gasLimit, "validation.gas_limit.invalid"),
        }),
      );
    } catch {
      return null;
    }
  }, [params.walletPrincipalText, wrapForm, wrapGasEstimateStatus]);

  function resetUnwrapNonceDeadline(): void {
    setUnwrapForm((current) => current);
  }

  return {
    unwrapForm,
    setUnwrapForm,
    wrapForm,
    setWrapForm,
    wrapPreviewRequestId,
    wrapGasEstimateStatus,
    wrapGasEstimateError,
    wrapNonceStatus,
    wrapNonceError,
    resetUnwrapNonceDeadline,
  };
}
