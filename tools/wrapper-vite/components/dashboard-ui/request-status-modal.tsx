// どこで: dashboard request status modal / 何を: request進捗と失敗時操作をモーダルで表示 / なぜ: 送信後の追跡導線を入力UIから分離するため

import type { ReactElement } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { dispatchBadgeVariant, executionBadgeVariant } from "@/lib/view";
import { deriveStatusPhase } from "@/lib/wrap-flow";
import type { StatusPanelView } from "./types";

function stepState(props: {
  current: number;
  index: number;
  failed: boolean;
}): string {
  if (props.failed && props.index === 3) {
    return "failed";
  }
  if (props.index < props.current) {
    return "done";
  }
  if (props.index === props.current) {
    return "active";
  }
  return "pending";
}

function phaseToStepIndex(status: StatusPanelView | null): number {
  const phase = deriveStatusPhase(status);
  if (phase === "idle") return 0;
  if (phase === "submitted") return 0;
  if (phase === "dispatching") return 1;
  if (phase === "executing") return 2;
  return 3;
}

export function RequestStatusModal(props: {
  open: boolean;
  requestId: string | null;
  status: StatusPanelView | null;
  statusLoading: boolean;
  message: string | null;
  walletConnected: boolean;
  retryLoading: boolean;
  withdrawLoading: boolean;
  onClose: () => void;
  onRetry: () => void;
  onWithdraw: () => void;
}): ReactElement | null {
  const currentStep = phaseToStepIndex(props.status);
  const failed = deriveStatusPhase(props.status) === "failed";

  if (!props.open) {
    return null;
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-zinc-950/40 px-4 py-6 backdrop-blur-sm"
      onClick={props.onClose}
    >
      <div
        className="w-full max-w-2xl rounded-2xl border border-emerald-100 bg-white shadow-2xl"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-4 border-b border-zinc-100 px-5 py-4">
          <div className="min-w-0">
            <p className="text-xs font-semibold uppercase tracking-[0.2em] text-emerald-700">
              Request Status
            </p>
            <p className="mt-2 break-all font-mono text-xs text-zinc-600">
              {props.requestId ?? "(request_id missing)"}
            </p>
          </div>
        </div>
        <div className="space-y-4 px-5 py-4">
          <ol className="grid grid-cols-4 gap-2 text-center text-xs">
            {["Submitted", "Dispatch", "Execution", "Final"].map((label, index) => {
              const state = stepState({
                current: currentStep,
                index,
                failed,
              });
              return (
                <li key={label} className="space-y-1">
                  <div
                    className={
                      state === "failed"
                        ? "mx-auto grid size-8 place-items-center rounded-full bg-rose-100 text-rose-700"
                        : state === "done"
                          ? "mx-auto grid size-8 place-items-center rounded-full bg-emerald-100 text-emerald-700"
                          : state === "active"
                            ? "mx-auto grid size-8 place-items-center rounded-full bg-sky-100 text-sky-700"
                            : "mx-auto grid size-8 place-items-center rounded-full bg-zinc-100 text-zinc-500"
                    }
                  >
                    {index + 1}
                  </div>
                  <span className="text-zinc-600">{label}</span>
                </li>
              );
            })}
          </ol>
          {props.statusLoading ? (
            <p className="rounded-lg bg-zinc-50 px-3 py-2 text-xs text-zinc-600">
              status を更新中...
            </p>
          ) : null}
          {props.message ? (
            <p className="rounded-lg bg-zinc-50 px-3 py-2 text-xs text-zinc-700">
              {props.message}
            </p>
          ) : null}
          {props.status ? (
            <div className="space-y-2 rounded-xl border border-zinc-200 bg-zinc-50/70 p-3 text-sm">
              <div className="flex items-center justify-between">
                <span>dispatch</span>
                <Badge variant={dispatchBadgeVariant(props.status.dispatchStatus)}>
                  {props.status.dispatchStatus ?? "null"}
                </Badge>
              </div>
              <div className="flex items-center justify-between">
                <span>execution</span>
                <Badge variant={executionBadgeVariant(props.status.executionStatus)}>
                  {props.status.executionStatus ?? "null"}
                </Badge>
              </div>
              <KeyValue label="ledger_tx_id" value={props.status.ledgerTxId} />
              <KeyValue label="error_code" value={props.status.errorCode} />
              <KeyValue label="withdrawn" value={String(props.status.withdrawn)} />
              <KeyValue label="withdraw_error_code" value={props.status.withdrawErrorCode} />
              {props.status.dispatchStatus !== null
              && props.status.executionStatus === "Failed"
              && !props.status.mintFailedRecoverable ? (
                <Button
                  variant="outline"
                  className="w-full"
                  onClick={props.onRetry}
                  disabled={props.retryLoading || !props.walletConnected}
                >
                  {props.retryLoading ? "Retrying..." : "Retry Failed Unwrap"}
                </Button>
              ) : null}
              {props.status.mintFailedRecoverable && !props.status.withdrawn ? (
                <Button
                  variant="outline"
                  className="w-full"
                  onClick={props.onWithdraw}
                  disabled={props.withdrawLoading || !props.walletConnected}
                >
                  {props.withdrawLoading ? "Withdrawing..." : "Withdraw Failed Wrap"}
                </Button>
              ) : null}
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function KeyValue(props: { label: string; value: string | null }): ReactElement {
  return (
    <div className="flex items-center justify-between gap-2">
      <span className="text-zinc-600">{props.label}</span>
      <span className="truncate font-mono text-xs text-zinc-800">
        {props.value ?? "null"}
      </span>
    </div>
  );
}
