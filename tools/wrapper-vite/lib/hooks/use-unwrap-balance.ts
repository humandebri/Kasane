// どこで: unwrap balance hook / 何を: 選択中 asset の wrapped token 残高を取得 / なぜ: unwrap 入力時も利用可能残高を即座に表示するため

import { useEffect, useState } from "react";
import { getLedgerDecimals } from "@/lib/canister/icrc2-client";
import { getUnwrapRequirements } from "@/lib/canister/wrap-client";
import { formatTokenBalance2 } from "@/lib/wrap-flow";
import { hexToBytes } from "@/lib/utils";

export function useUnwrapBalance(params: {
  assetId: string;
  callerEvmAddressHex: string | null;
}) {
  const [balanceText, setBalanceText] = useState<string | null>(null);
  const [balanceValue, setBalanceValue] = useState<bigint | null>(null);
  const [decimals, setDecimals] = useState<number | null>(null);

  useEffect(() => {
    const assetId = params.assetId.trim();
    const callerEvmAddressHex = params.callerEvmAddressHex;
    if (assetId === "" || callerEvmAddressHex === null) {
      setBalanceText(null);
      setBalanceValue(null);
      setDecimals(null);
      return;
    }

    let cancelled = false;
    void Promise.all([
      getUnwrapRequirements({
        assetId,
        amountE8s: 0n,
        callerEvmAddress: hexToBytes(callerEvmAddressHex),
      }),
      getLedgerDecimals(assetId),
    ])
      .then(([requirements, decimals]) => {
        if (cancelled) {
          return;
        }
        setBalanceText(formatTokenBalance2(requirements.balance, decimals));
        setBalanceValue(requirements.balance);
        setDecimals(decimals);
      })
      .catch(() => {
        if (!cancelled) {
          setBalanceText(null);
          setBalanceValue(null);
          setDecimals(null);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [params.assetId, params.callerEvmAddressHex]);

  return { balanceText, balanceValue, decimals };
}
