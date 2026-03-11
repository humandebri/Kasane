// どこで: dashboard status panel / 何を: dispatch/execution追跡と回収導線を表示 / なぜ: 送信後の状態把握と失敗時対応を即時に行うため

import { RefreshCcw } from "lucide-react";
import type { ReactElement } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
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

export function StatusPanel(props: {
  requestIdInput: string;
  status: StatusPanelView | null;
  statusLoading: boolean;
  message: string | null;
  walletConnected: boolean;
  retryLoading: boolean;
  withdrawLoading: boolean;
  onChangeRequestId: (value: string) => void;
  onQuery: () => void;
  onRetry: () => void;
  onWithdraw: () => void;
}): ReactElement {
  const currentStep = phaseToStepIndex(props.status);
  const failed = deriveStatusPhase(props.status) === "failed";

  return (
    <Card className="rounded-2xl border-emerald-100">
      <CardHeader>
        <CardTitle>Status</CardTitle>
        <CardDescription>
          request_id または unwrap の tx_id を追跡し、dispatch/executionを自動更新します。
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex gap-2">
          <Input
            placeholder="0x... (request_id / tx_id)"
            value={props.requestIdInput}
            onChange={(event) => props.onChangeRequestId(event.target.value)}
          />
          <Button
            variant="secondary"
            onClick={props.onQuery}
            disabled={props.statusLoading || props.requestIdInput.trim() === ""}
          >
            <RefreshCcw className="mr-1 size-4" />
            Query
          </Button>
        </div>
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
                      ? "mx-auto size-7 rounded-full bg-rose-100 text-rose-700 grid place-items-center"
                      : state === "done"
                        ? "mx-auto size-7 rounded-full bg-emerald-100 text-emerald-700 grid place-items-center"
                        : state === "active"
                          ? "mx-auto size-7 rounded-full bg-sky-100 text-sky-700 grid place-items-center"
                          : "mx-auto size-7 rounded-full bg-zinc-100 text-zinc-500 grid place-items-center"
                  }
                >
                  {index + 1}
                </div>
                <span className="text-zinc-600">{label}</span>
              </li>
            );
          })}
        </ol>
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
            <KeyValue
              label="withdraw_error_code"
              value={props.status.withdrawErrorCode}
            />
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
      </CardContent>
    </Card>
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
