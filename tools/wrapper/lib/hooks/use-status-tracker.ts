// どこで: wrapper dashboard hook / 何を: status照会と自動ポーリング停止制御を提供 / なぜ: 通信失敗時の無限再試行を防ぐため

import { useCallback, useEffect, useState } from "react";
import {
  getDispatchResult,
  getUnwrapRequestIdsByTxId,
} from "@/lib/canister/wrapper-client";
import { getExecutionResult } from "@/lib/canister/wrap-client";
import { mergeStatus } from "@/lib/merge";
import {
  DEFAULT_MAX_POLL_FAILURES,
  DEFAULT_POLL_INTERVAL_MS,
  messageAfterRefreshSuccess,
  nextPollFailureState,
  shouldScheduleAutoPolling,
} from "@/lib/status-poll";
import type { StatusResponse } from "@/lib/types";
import { bytesToHex, parseRequestIdHex } from "@/lib/utils";
import { isTerminalStatus } from "@/lib/wrap-flow";

async function resolveTrackingRequestId(inputId: Uint8Array): Promise<Uint8Array> {
  const unwrapRequestIds = await getUnwrapRequestIdsByTxId(inputId);
  if (unwrapRequestIds.length > 1) {
    throw new Error("status.unwrap_tx.multiple_request_ids");
  }
  return unwrapRequestIds[0] ?? inputId;
}

export function useStatusTracker() {
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [statusLoading, setStatusLoading] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [autoPolling, setAutoPolling] = useState(false);
  const [pollFailureCount, setPollFailureCount] = useState(0);

  const refreshStatus = useCallback(
    async (requestIdHex: string, background = false): Promise<boolean> => {
      if (!background) {
        setStatusLoading(true);
      }
      try {
        const inputId = parseRequestIdHex(requestIdHex.trim());
        const requestId = await resolveTrackingRequestId(inputId);
        const resolvedRequestIdHex = bytesToHex(requestId);
        const [dispatchResult, executionResult] = await Promise.all([
          getDispatchResult(requestId),
          getExecutionResult(requestId),
        ]);
        setStatus(
          mergeStatus({
            requestIdHex: resolvedRequestIdHex,
            dispatchResult,
            executionResult,
          }),
        );
        setPollFailureCount(0);
        setMessage((current) =>
          messageAfterRefreshSuccess({ currentMessage: current, background }),
        );
        return true;
      } catch (error) {
        if (!background) {
          setStatus(null);
          setMessage(error instanceof Error ? error.message : "status_failed");
        }
        return false;
      } finally {
        if (!background) {
          setStatusLoading(false);
        }
      }
    },
    [],
  );

  useEffect(() => {
    if (
      !shouldScheduleAutoPolling({
        autoPolling,
        status,
        pollFailureCount,
        maxFailures: DEFAULT_MAX_POLL_FAILURES,
      })
    ) {
      return;
    }
    const currentStatus = status;
    if (!currentStatus) {
      return;
    }
    const timer = window.setTimeout(() => {
      void (async () => {
        const ok = await refreshStatus(currentStatus.requestId, true);
        if (ok) {
          return;
        }
        setPollFailureCount((current) => {
          const next = nextPollFailureState({
            currentFailureCount: current,
            maxFailures: DEFAULT_MAX_POLL_FAILURES,
          });
          if (next.shouldStop) {
            setAutoPolling(false);
            setMessage("status.auto_poll_stopped");
          }
          return next.nextFailureCount;
        });
      })();
    }, DEFAULT_POLL_INTERVAL_MS);
    return () => window.clearTimeout(timer);
  }, [autoPolling, pollFailureCount, refreshStatus, status]);

  useEffect(() => {
    if (autoPolling && isTerminalStatus(status)) {
      setAutoPolling(false);
    }
  }, [autoPolling, status]);

  return {
    status,
    setStatus,
    statusLoading,
    message,
    setMessage,
    autoPolling,
    setAutoPolling,
    pollFailureCount,
    refreshStatus,
  };
}
