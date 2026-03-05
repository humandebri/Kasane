// どこで: status polling共通 / 何を: ポーリング継続判定と失敗カウント遷移を提供 / なぜ: hook外で回帰テスト可能にするため

import type { StatusResponse } from "./types";
import { isTerminalStatus } from "./wrap-flow";

export const DEFAULT_POLL_INTERVAL_MS = 2_000;
export const DEFAULT_MAX_POLL_FAILURES = 3;

export function shouldScheduleAutoPolling(args: {
  autoPolling: boolean;
  status: StatusResponse | null;
  pollFailureCount: number;
  maxFailures?: number;
}): boolean {
  const maxFailures = args.maxFailures ?? DEFAULT_MAX_POLL_FAILURES;
  return (
    args.autoPolling &&
    args.status !== null &&
    !isTerminalStatus(args.status) &&
    args.pollFailureCount < maxFailures
  );
}

export function nextPollFailureState(args: {
  currentFailureCount: number;
  maxFailures?: number;
}): { nextFailureCount: number; shouldStop: boolean } {
  const maxFailures = args.maxFailures ?? DEFAULT_MAX_POLL_FAILURES;
  const nextFailureCount = args.currentFailureCount + 1;
  return {
    nextFailureCount,
    shouldStop: nextFailureCount >= maxFailures,
  };
}

export function messageAfterRefreshSuccess(args: {
  currentMessage: string | null;
  background: boolean;
}): string | null {
  if (!args.background) {
    return null;
  }
  if (args.currentMessage === "status.auto_poll_stopped") {
    return null;
  }
  return args.currentMessage;
}
