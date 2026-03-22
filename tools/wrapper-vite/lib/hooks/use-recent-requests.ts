// どこで: Recent Requests hook / 何を: Juno から principal ごとの履歴を取得・保存する / なぜ: dashboard から session memory を外して永続化するため

import { useCallback, useEffect, useRef, useState } from "react";
import type { Identity } from "@icp-sdk/core/agent";
import type { HistoryEntry } from "@/components/dashboard-ui/types";
import { listRecentRequests, saveRecentRequest } from "@/lib/canister/recent-requests-client";
import { mergeRecentRequestHistory } from "@/lib/recent-requests";

export function createRecentRequestsScopeKey(args: {
  principalText: string | null;
  satelliteId: string | null;
}): string {
  return `${args.principalText ?? ""}::${args.satelliteId ?? ""}`;
}

export function shouldApplyRecentRequestsResult(args: {
  startedScopeKey: string;
  currentScopeKey: string;
  startedRefreshSeq?: number;
  currentRefreshSeq?: number;
}): boolean {
  if (args.startedScopeKey !== args.currentScopeKey) {
    return false;
  }
  if (args.startedRefreshSeq !== undefined || args.currentRefreshSeq !== undefined) {
    return args.startedRefreshSeq === args.currentRefreshSeq;
  }
  return true;
}

export function useRecentRequests(params: {
  principalText: string | null;
  getIdentity: () => Promise<Identity | null>;
  satelliteId: string | null;
}) {
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refreshSeqRef = useRef(0);
  const currentScopeKeyRef = useRef(
    createRecentRequestsScopeKey({
      principalText: params.principalText,
      satelliteId: params.satelliteId,
    }),
  );
  currentScopeKeyRef.current = createRecentRequestsScopeKey({
    principalText: params.principalText,
    satelliteId: params.satelliteId,
  });

  const refresh = useCallback(async (): Promise<void> => {
    refreshSeqRef.current += 1;
    const refreshSeq = refreshSeqRef.current;
    const scopeKey = currentScopeKeyRef.current;
    if (!params.principalText) {
      setHistory([]);
      setLoading(false);
      setError(null);
      return;
    }
    if (!params.satelliteId) {
      setHistory([]);
      setLoading(false);
      setError("history.satellite_id_missing");
      return;
    }
    setLoading(true);
    try {
      const identity = await params.getIdentity();
      if (identity === null) {
        throw new Error("wallet.not_connected");
      }
      const nextHistory = await listRecentRequests(
        identity,
        params.principalText,
        params.satelliteId,
      );
      if (!shouldApplyRecentRequestsResult({
        startedScopeKey: scopeKey,
        currentScopeKey: currentScopeKeyRef.current,
        startedRefreshSeq: refreshSeq,
        currentRefreshSeq: refreshSeqRef.current,
      })) {
        return;
      }
      setHistory(nextHistory);
      setError(null);
    } catch (nextError) {
      if (!shouldApplyRecentRequestsResult({
        startedScopeKey: scopeKey,
        currentScopeKey: currentScopeKeyRef.current,
        startedRefreshSeq: refreshSeq,
        currentRefreshSeq: refreshSeqRef.current,
      })) {
        return;
      }
      setHistory([]);
      setError(nextError instanceof Error ? nextError.message : "history.load_failed");
    } finally {
      if (shouldApplyRecentRequestsResult({
        startedScopeKey: scopeKey,
        currentScopeKey: currentScopeKeyRef.current,
        startedRefreshSeq: refreshSeq,
        currentRefreshSeq: refreshSeqRef.current,
      })) {
        setLoading(false);
      }
    }
  }, [params.getIdentity, params.principalText, params.satelliteId]);

  const save = useCallback(async (entry: HistoryEntry): Promise<void> => {
    if (!params.principalText || !params.satelliteId) {
      return;
    }
    const scopeKey = currentScopeKeyRef.current;
    try {
      const identity = await params.getIdentity();
      if (identity === null) {
        throw new Error("wallet.not_connected");
      }
      const saved = await saveRecentRequest(
        identity,
        params.principalText,
        params.satelliteId,
        entry,
      );
      if (!shouldApplyRecentRequestsResult({
        startedScopeKey: scopeKey,
        currentScopeKey: currentScopeKeyRef.current,
      })) {
        return;
      }
      setHistory((current) => mergeRecentRequestHistory(current, saved));
      setError(null);
    } catch (nextError) {
      if (!shouldApplyRecentRequestsResult({
        startedScopeKey: scopeKey,
        currentScopeKey: currentScopeKeyRef.current,
      })) {
        return;
      }
      setError(nextError instanceof Error ? nextError.message : "history.save_failed");
    }
  }, [params.getIdentity, params.principalText, params.satelliteId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return {
    history,
    loading,
    error,
    refresh,
    save,
  };
}
