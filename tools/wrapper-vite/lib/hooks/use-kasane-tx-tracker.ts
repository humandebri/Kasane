// どこで: MetaMask tx tracker / 何を: Kasane RPC 経由で eth receipt を追跡する / なぜ: request_id 非対応の MetaMask unwrap を tx 単位で可視化するため

import { useCallback, useEffect, useState } from "react";
import { getKasaneTransactionStatus, type KasaneTransactionStatus } from "@/lib/kasane-rpc";

const DEFAULT_TX_POLL_INTERVAL_MS = 5_000;
const DEFAULT_MAX_TX_POLL_FAILURES = 5;

export function useKasaneTxTracker(args: {
  rpcUrl: string | null;
  explorerBaseUrl: string | null;
}) {
  const [transaction, setTransaction] = useState<KasaneTransactionStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [autoPolling, setAutoPolling] = useState(false);
  const [pollFailureCount, setPollFailureCount] = useState(0);

  const refreshTransaction = useCallback(async (transactionHash: string, background = false): Promise<boolean> => {
    if (args.rpcUrl === null) {
      setMessage("kasane.rpc_missing");
      return false;
    }
    if (!background) {
      setLoading(true);
    }
    try {
      const nextStatus = await getKasaneTransactionStatus({
        rpcUrl: args.rpcUrl,
        transactionHash,
        explorerBaseUrl: args.explorerBaseUrl,
      });
      setTransaction(nextStatus);
      setPollFailureCount(0);
      if (!background) {
        setMessage(null);
      }
      return true;
    } catch (error) {
      if (!background) {
        setMessage(error instanceof Error ? error.message : "kasane.tx_status_failed");
      }
      return false;
    } finally {
      if (!background) {
        setLoading(false);
      }
    }
  }, [args.explorerBaseUrl, args.rpcUrl]);

  useEffect(() => {
    if (!autoPolling || transaction === null || transaction.transactionStatus !== "Pending") {
      return;
    }
    if (pollFailureCount >= DEFAULT_MAX_TX_POLL_FAILURES) {
      setAutoPolling(false);
      setMessage("kasane.tx_auto_poll_stopped");
      return;
    }
    const timer = window.setTimeout(() => {
      void refreshTransaction(transaction.transactionHash, true).then((ok) => {
        if (ok) {
          return;
        }
        setPollFailureCount((current) => current + 1);
      });
    }, DEFAULT_TX_POLL_INTERVAL_MS);
    return () => window.clearTimeout(timer);
  }, [autoPolling, pollFailureCount, refreshTransaction, transaction]);

  return {
    transaction,
    setTransaction,
    loading,
    message,
    setMessage,
    autoPolling,
    setAutoPolling,
    refreshTransaction,
  };
}
