// どこで: wrapper dashboard hook / 何を: form state と request_id preview を管理 / なぜ: 画面部品と送信ロジックの責務を分離するため

import { useEffect, useMemo, useState } from "react";
import type { UnwrapFormState, WrapFormState } from "@/components/dashboard-ui/types";
import { callerEvmAddressFromPrincipalText, principalTextToBytes } from "@/lib/principal";
import {
  decimalToBytes32,
  deriveRequestId,
  deriveWrapRequestId,
} from "@/lib/request-id";
import { bytesToHex, hexToBytes } from "@/lib/utils";
import {
  defaultDeadlineText,
  parsePositiveBigInt,
  parseU64,
  randomU64NonceText,
} from "@/lib/wrap-input";

export function useWrapperForms(params: {
  walletPrincipalText: string | null;
  wrapCanisterId: string;
}) {
  const [unwrapForm, setUnwrapForm] = useState<UnwrapFormState>({
    assetId: "",
    amount: "",
    recipient: "",
    userNonce: randomU64NonceText(),
    deadline: defaultDeadlineText(),
  });
  const [wrapForm, setWrapForm] = useState<WrapFormState>({
    assetId: "",
    amount: "",
    evmRecipient: "",
    evmNonce: randomU64NonceText(),
    gasLimit: "300000",
  });

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

  const unwrapPreviewRequestId = useMemo(() => {
    if (!params.walletPrincipalText) {
      return null;
    }
    try {
      return bytesToHex(
        deriveRequestId({
          callerEvmAddress: callerEvmAddressFromPrincipalText(
            params.walletPrincipalText,
          ),
          vaultCanisterId: params.wrapCanisterId,
          assetId: unwrapForm.assetId.trim(),
          amount: parsePositiveBigInt(unwrapForm.amount, "validation.amount.invalid"),
          recipient: unwrapForm.recipient.trim(),
          userNonce: parseU64(unwrapForm.userNonce, "validation.user_nonce.invalid"),
          deadline: parseU64(unwrapForm.deadline, "validation.deadline.invalid"),
        }),
      );
    } catch {
      return null;
    }
  }, [params.walletPrincipalText, params.wrapCanisterId, unwrapForm]);

  const wrapPreviewRequestId = useMemo(() => {
    if (!params.walletPrincipalText) {
      return null;
    }
    try {
      return bytesToHex(
        deriveWrapRequestId({
          fromOwner: principalTextToBytes(params.walletPrincipalText),
          assetId: principalTextToBytes(wrapForm.assetId.trim()),
          amount: decimalToBytes32(wrapForm.amount.trim()),
          evmRecipient: hexToBytes(wrapForm.evmRecipient.trim()),
          evmNonce: parseU64(wrapForm.evmNonce, "validation.evm_nonce.invalid"),
          gasLimit: parseU64(wrapForm.gasLimit, "validation.gas_limit.invalid"),
        }),
      );
    } catch {
      return null;
    }
  }, [params.walletPrincipalText, wrapForm]);

  function resetUnwrapNonceDeadline(): void {
    setUnwrapForm((current) => ({
      ...current,
      userNonce: randomU64NonceText(),
      deadline: defaultDeadlineText(),
    }));
  }

  return {
    unwrapForm,
    setUnwrapForm,
    wrapForm,
    setWrapForm,
    unwrapPreviewRequestId,
    wrapPreviewRequestId,
    resetUnwrapNonceDeadline,
  };
}
