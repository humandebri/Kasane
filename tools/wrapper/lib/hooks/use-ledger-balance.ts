// どこで: wrapper ledger balance hook / 何を: 選択中ledgerの残高とdecimalsを並列取得する / なぜ: amount入力前に利用可能枚数を即座に示すため

import { useCallback, useEffect, useRef, useState } from "react";
import { getLedgerBalance, getLedgerDecimals } from "@/lib/canister/icrc2-client";
import { formatTokenBalance2 } from "@/lib/wrap-flow";

export async function readLedgerBalance(args: {
  ledgerCanisterId: string;
  ownerPrincipalText: string;
}): Promise<{
  balanceText: string;
  balanceValue: bigint;
  decimals: number;
}> {
  const [balanceValue, decimals] = await Promise.all([
    getLedgerBalance(args),
    getLedgerDecimals(args.ledgerCanisterId),
  ]);
  return {
    balanceText: formatTokenBalance2(balanceValue, decimals),
    balanceValue,
    decimals,
  };
}

export function useLedgerBalance(params: {
  ledgerCanisterId: string;
  ownerPrincipalText: string | null;
}) {
  const [balanceText, setBalanceText] = useState<string | null>(null);
  const [balanceValue, setBalanceValue] = useState<bigint | null>(null);
  const [decimals, setDecimals] = useState<number | null>(null);
  const [refreshNonce, setRefreshNonce] = useState(0);
  const cachedLedgerCanisterId = useRef<string | null>(null);
  const cachedDecimals = useRef<number | null>(null);

  const refresh = useCallback(async (): Promise<void> => {
    setRefreshNonce((current) => current + 1);
  }, []);

  useEffect(() => {
    const ledgerCanisterId = params.ledgerCanisterId.trim();
    const ownerPrincipalText = params.ownerPrincipalText;
    if (ledgerCanisterId === "" || ownerPrincipalText === null) {
      setBalanceText(null);
      setBalanceValue(null);
      setDecimals(null);
      cachedLedgerCanisterId.current = null;
      cachedDecimals.current = null;
      return;
    }

    let cancelled = false;
    const shouldReuseDecimals =
      cachedLedgerCanisterId.current === ledgerCanisterId
      && cachedDecimals.current !== null
      && refreshNonce > 0;
    const nextBalance = shouldReuseDecimals
      ? getLedgerBalance({ ledgerCanisterId, ownerPrincipalText }).then((nextBalanceValue) => ({
          balanceText: formatTokenBalance2(nextBalanceValue, cachedDecimals.current ?? 0),
          balanceValue: nextBalanceValue,
          decimals: cachedDecimals.current ?? 0,
        }))
      : readLedgerBalance({
          ledgerCanisterId,
          ownerPrincipalText,
        });
    void nextBalance
      .then((next) => {
        if (cancelled) {
          return;
        }
        cachedLedgerCanisterId.current = ledgerCanisterId;
        cachedDecimals.current = next.decimals;
        setBalanceText(next.balanceText);
        setBalanceValue(next.balanceValue);
        setDecimals(next.decimals);
      })
      .catch(() => {
        if (!cancelled) {
          cachedLedgerCanisterId.current = null;
          cachedDecimals.current = null;
          setBalanceText(null);
          setBalanceValue(null);
          setDecimals(null);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [params.ledgerCanisterId, params.ownerPrincipalText, refreshNonce]);

  return { balanceText, balanceValue, decimals, refresh };
}
