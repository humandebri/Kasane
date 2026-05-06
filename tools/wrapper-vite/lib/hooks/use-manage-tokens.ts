// どこで: Manage Tokens hook
// 何を: token list endpoint の取得・refresh・selector option 化を担当
// なぜ: drawer UI と compact selector が同じ token source を共有するため

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { resolveLedgerQueryHost } from "@/lib/asset-catalog";
import { getLedgerBalance, getLedgerDecimals } from "@/lib/canister/icrc2-client";
import { normalizeIcpTokenList, toManageTokenOptions, type ManageTokenRow } from "@/lib/icp-token-list";
import { formatTokenBalance2 } from "@/lib/wrap-flow";

export function useManageTokens(tokenListUrl: string | null, ownerPrincipalText: string | null) {
  const [rows, setRows] = useState<ManageTokenRow[]>([]);
  const [balanceMap, setBalanceMap] = useState<Record<string, string | null>>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refreshSeqRef = useRef(0);

  const refresh = useCallback(async (): Promise<void> => {
    refreshSeqRef.current += 1;
    const refreshSeq = refreshSeqRef.current;
    if (tokenListUrl === null || tokenListUrl.trim() === "") {
      setRows([]);
      setBalanceMap({});
      setError("config.missing:VITE_ICP_TOKEN_LIST_URL");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const response = await fetch(tokenListUrl, { method: "GET" });
      if (!response.ok) {
        throw new Error(`token_list.fetch_failed:${response.status}`);
      }
      const payload: unknown = await response.json();
      const nextRows = normalizeIcpTokenList(payload);
      if (refreshSeq !== refreshSeqRef.current) {
        return;
      }
      setRows(nextRows);
      setBalanceMap({});
      setError(null);
    } catch (nextError) {
      if (refreshSeq !== refreshSeqRef.current) {
        return;
      }
      setRows([]);
      setBalanceMap({});
      setError(nextError instanceof Error ? nextError.message : "token_list.fetch_failed:unknown");
    } finally {
      if (refreshSeq === refreshSeqRef.current) {
        setLoading(false);
      }
    }
  }, [tokenListUrl]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (ownerPrincipalText === null) {
      setBalanceMap({});
      return;
    }

    let cancelled = false;
    void Promise.all(rows.map(async (row) => {
      try {
        const [balance, decimals] = await Promise.all([
          getLedgerBalance({
            ledgerCanisterId: row.assetId,
            ownerPrincipalText,
            queryHost: resolveLedgerQueryHost(row.assetId) ?? undefined,
          }),
          getLedgerDecimals(row.assetId, resolveLedgerQueryHost(row.assetId) ?? undefined),
        ]);
        return {
          assetId: row.assetId,
          balanceText: formatTokenBalance2(balance, decimals),
        };
      } catch {
        return {
          assetId: row.assetId,
          balanceText: null,
        };
      }
    })).then((balances) => {
      if (cancelled) {
        return;
      }
      setBalanceMap(Object.fromEntries(
        balances.map((entry) => [entry.assetId, entry.balanceText]),
      ));
    });

    return () => {
      cancelled = true;
    };
  }, [ownerPrincipalText, rows]);

  const rowsWithBalances = useMemo(
    () => rows.map((row) => ({
      ...row,
      balanceText: balanceMap[row.assetId] ?? null,
    })),
    [balanceMap, rows],
  );

  const assetOptions = useMemo(() => toManageTokenOptions(rowsWithBalances), [rowsWithBalances]);

  return {
    rows: rowsWithBalances,
    loading,
    error,
    assetOptions,
    refresh,
  };
}
