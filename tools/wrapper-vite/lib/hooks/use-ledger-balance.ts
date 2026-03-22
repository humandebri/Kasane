// どこで: wrapper ledger balance hook / 何を: 選択中ledgerの残高とdecimalsを並列取得する / なぜ: amount入力前に利用可能枚数を即座に示すため

import { useEffect, useState } from "react";
import { getLedgerBalance, getLedgerDecimals } from "@/lib/canister/icrc2-client";
import { formatTokenBalance2 } from "@/lib/wrap-flow";

export function useLedgerBalance(params: {
  ledgerCanisterId: string;
  ownerPrincipalText: string | null;
}) {
  const [balanceText, setBalanceText] = useState<string | null>(null);
  const [balanceValue, setBalanceValue] = useState<bigint | null>(null);
  const [decimals, setDecimals] = useState<number | null>(null);

  useEffect(() => {
    const ledgerCanisterId = params.ledgerCanisterId.trim();
    const ownerPrincipalText = params.ownerPrincipalText;
    if (ledgerCanisterId === "" || ownerPrincipalText === null) {
      setBalanceText(null);
      setBalanceValue(null);
      setDecimals(null);
      return;
    }

    let cancelled = false;
    void Promise.all([
      getLedgerBalance({ ledgerCanisterId, ownerPrincipalText }),
      getLedgerDecimals(ledgerCanisterId),
    ])
      .then(([balance, decimals]) => {
        if (cancelled) {
          return;
        }
        setBalanceText(formatTokenBalance2(balance, decimals));
        setBalanceValue(balance);
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
  }, [params.ledgerCanisterId, params.ownerPrincipalText]);

  return { balanceText, balanceValue, decimals };
}
