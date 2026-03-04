"use client";

// どこで: wrapperダッシュボード / 何を: unwrap submit・status照会・履歴表示を提供 / なぜ: 現行サポート範囲を明示して誤認を防ぐため

import { useState, type Dispatch, type SetStateAction } from "react";
import type { StatusResponse, SubmitPayload, SubmitResponse } from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { dispatchBadgeVariant, executionBadgeVariant } from "@/lib/view";

type HistoryEntry = {
  requestId: string;
  submittedAt: string;
};

type SubmitState = {
  loading: boolean;
  result: SubmitResponse | null;
  error: string | null;
};

const EMPTY_FORM = {
  assetId: "",
  amount: "",
  recipient: "",
};

export function WrapperDashboard() {
  const [form, setForm] = useState(EMPTY_FORM);
  const [submitState, setSubmitState] = useState<SubmitState>({
    loading: false,
    result: null,
    error: null,
  });
  const [queryRequestId, setQueryRequestId] = useState("");
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [statusLoading, setStatusLoading] = useState(false);
  const [withdrawLoading, setWithdrawLoading] = useState(false);
  const [withdrawMessage, setWithdrawMessage] = useState<string | null>(null);
  const [history, setHistory] = useState<HistoryEntry[]>([]);

  async function onSubmit(): Promise<void> {
    setSubmitState({ loading: true, result: null, error: null });
    try {
      const payload: SubmitPayload = {
        assetId: form.assetId.trim(),
        amount: form.amount.trim(),
        recipient: form.recipient.trim(),
      };
      const response = await fetch("/api/wrap/submit", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload),
      });
      const body = await response.json();
      if (!response.ok) {
        const errorCode = typeof body.errorCode === "string" ? body.errorCode : "submit_failed";
        throw new Error(errorCode);
      }
      const data = body as SubmitResponse;
      setSubmitState({ loading: false, result: data, error: null });
      setQueryRequestId(data.requestId);
      setHistory((prev) => {
        const next = [{ requestId: data.requestId, submittedAt: new Date().toISOString() }, ...prev.filter((v) => v.requestId !== data.requestId)];
        return next.slice(0, 20);
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : "submit_failed";
      setSubmitState({ loading: false, result: null, error: message });
    }
  }

  async function queryStatus(requestId: string): Promise<void> {
    setStatusLoading(true);
    setStatusError(null);
    setStatus(null);
    setWithdrawMessage(null);
    try {
      const response = await fetch(`/api/wrap/status/${encodeURIComponent(requestId)}`);
      const body = await response.json();
      if (!response.ok) {
        const errorCode = typeof body.errorCode === "string" ? body.errorCode : "status_failed";
        throw new Error(errorCode);
      }
      setStatus(body as StatusResponse);
    } catch (error) {
      const message = error instanceof Error ? error.message : "status_failed";
      setStatusError(message);
    } finally {
      setStatusLoading(false);
    }
  }

  async function onWithdraw(requestId: string): Promise<void> {
    setWithdrawLoading(true);
    setWithdrawMessage(null);
    try {
      const response = await fetch("/api/wrap/withdraw", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ requestId }),
      });
      const body = await response.json();
      if (!response.ok) {
        const errorCode =
          typeof body.errorCode === "string" ? body.errorCode : "withdraw_failed";
        throw new Error(errorCode);
      }
      const ledgerTxId =
        typeof body.ledgerTxId === "string" ? body.ledgerTxId : "unknown";
      setWithdrawMessage(`withdraw succeeded: ${ledgerTxId}`);
      await queryStatus(requestId);
    } catch (error) {
      const message = error instanceof Error ? error.message : "withdraw_failed";
      setWithdrawMessage(`withdraw error: ${message}`);
    } finally {
      setWithdrawLoading(false);
    }
  }

  return (
    <main className="mx-auto flex min-h-screen w-full max-w-6xl flex-col gap-6 px-4 py-8 sm:px-8">
      <header className="space-y-2">
        <p className="text-sm font-medium text-emerald-700">Kasane Wrapper Dashboard</p>
        <h1 className="text-3xl font-semibold tracking-tight">unwrap submit + wrap recovery</h1>
        <p className="text-sm text-zinc-600">dispatch status (wrapper) と execution status (wrap canister) を分離表示し、mint失敗時は withdraw 導線を表示します。</p>
      </header>

      <section className="grid gap-6 lg:grid-cols-[1.25fr_1fr]">
        <Card>
          <CardHeader>
            <CardTitle>1. Unwrap Submit</CardTitle>
            <CardDescription>送信後に request_id を表示します。ステータス確定は照会で確認します。</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <FormFields form={form} setForm={setForm} />
            <Button onClick={() => void onSubmit()} disabled={submitState.loading} className="w-full">
              {submitState.loading ? "送信中..." : "Unwrapを送信"}
            </Button>
            {submitState.result ? (
              <div className="rounded-md border border-emerald-200 bg-emerald-50 p-3 text-sm">
                <div className="font-medium text-emerald-900">request_id: {submitState.result.requestId}</div>
                <div className="mt-1 text-emerald-800">dispatch status (wrapper): {submitState.result.dispatchStatus}</div>
              </div>
            ) : null}
            {submitState.error ? <p className="text-sm text-rose-700">error: {submitState.error}</p> : null}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>2. Request Status</CardTitle>
            <CardDescription>request_id を入力して dispatch/execution を照会し、必要時は withdraw できます。</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <Input placeholder="0x... (32 bytes)" value={queryRequestId} onChange={(event) => setQueryRequestId(event.target.value)} />
            <Button variant="secondary" onClick={() => void queryStatus(queryRequestId.trim())} disabled={statusLoading || queryRequestId.trim() === ""}>
              {statusLoading ? "照会中..." : "ステータス照会"}
            </Button>
            {statusError ? <p className="text-sm text-rose-700">error: {statusError}</p> : null}
            {status ? (
              <div className="space-y-2 rounded-md border border-zinc-200 p-3 text-sm">
                <div className="flex items-center justify-between gap-3">
                  <span className="text-zinc-600">dispatch status (wrapper)</span>
                  <Badge variant={dispatchBadgeVariant(status.dispatchStatus)}>{status.dispatchStatus ?? "null"}</Badge>
                </div>
                <div className="flex items-center justify-between gap-3">
                  <span className="text-zinc-600">execution status (wrap canister)</span>
                  <Badge variant={executionBadgeVariant(status.executionStatus)}>{status.executionStatus ?? "null"}</Badge>
                </div>
                <KeyValue label="vault_canister_id" value={status.vaultCanisterId} />
                <KeyValue label="ledger_tx_id" value={status.ledgerTxId} />
                <KeyValue label="error_code" value={status.errorCode} />
                <KeyValue label="mint_failed_recoverable" value={String(status.mintFailedRecoverable)} />
                <KeyValue label="withdrawn" value={String(status.withdrawn)} />
                <KeyValue label="withdraw_ledger_tx_id" value={status.withdrawLedgerTxId} />
                <KeyValue label="withdraw_error_code" value={status.withdrawErrorCode} />
                {status.mintFailedRecoverable && !status.withdrawn ? (
                  <Button
                    variant="outline"
                    onClick={() => void onWithdraw(status.requestId)}
                    disabled={withdrawLoading}
                    className="mt-2 w-full"
                  >
                    {withdrawLoading ? "Withdraw中..." : "Withdraw"}
                  </Button>
                ) : null}
                {withdrawMessage ? (
                  <p className="text-xs text-zinc-700">{withdrawMessage}</p>
                ) : null}
              </div>
            ) : null}
          </CardContent>
        </Card>
      </section>

      <Card>
        <CardHeader>
          <CardTitle>3. Recent Requests (max 20)</CardTitle>
          <CardDescription>送信成功した request_id のセッション履歴です。クリックで再照会します。</CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>request_id</TableHead>
                <TableHead>submitted_at</TableHead>
                <TableHead className="text-right">action</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {history.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={3} className="text-center text-zinc-500">
                    まだ履歴はありません
                  </TableCell>
                </TableRow>
              ) : (
                history.map((item) => (
                  <TableRow key={item.requestId}>
                    <TableCell className="font-mono text-xs">{item.requestId}</TableCell>
                    <TableCell className="text-xs text-zinc-600">{item.submittedAt}</TableCell>
                    <TableCell className="text-right">
                      <Button size="sm" variant="outline" onClick={() => {
                        setQueryRequestId(item.requestId);
                        void queryStatus(item.requestId);
                      }}>
                        再照会
                      </Button>
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </main>
  );
}

function FormFields({
  form,
  setForm,
}: {
  form: { assetId: string; amount: string; recipient: string };
  setForm: Dispatch<SetStateAction<{ assetId: string; amount: string; recipient: string }>>;
}) {
  return (
    <div className="space-y-3">
      <LabeledInput label="assetId" value={form.assetId} onChange={(value) => setForm((prev) => ({ ...prev, assetId: value }))} />
      <LabeledInput label="amount" value={form.amount} onChange={(value) => setForm((prev) => ({ ...prev, amount: value }))} placeholder="1000000000000000000" />
      <LabeledInput label="recipient" value={form.recipient} onChange={(value) => setForm((prev) => ({ ...prev, recipient: value }))} />
    </div>
  );
}

function LabeledInput({
  label,
  value,
  onChange,
  placeholder,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}) {
  return (
    <label className="block space-y-1">
      <span className="text-xs font-medium text-zinc-600">{label}</span>
      <Input value={value} onChange={(event) => onChange(event.target.value)} placeholder={placeholder} />
    </label>
  );
}

function KeyValue({ label, value }: { label: string; value: string | null }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-zinc-600">{label}</span>
      <span className="max-w-[65%] truncate font-mono text-xs">{value ?? "null"}</span>
    </div>
  );
}
