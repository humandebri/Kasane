// どこで: dashboard history panel / 何を: 直近request履歴と手動request_id照会を表示 / なぜ: リロード後も再照会しやすくするため

import { useState, type ReactElement } from "react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { parseRequestIdHex } from "@/lib/utils";
import type { HistoryEntry } from "./types";

export function HistoryPanel(props: {
  history: HistoryEntry[];
  loading: boolean;
  error: string | null;
  walletConnected: boolean;
  onOpen: (requestId: string) => void;
}): ReactElement {
  const [requestIdInput, setRequestIdInput] = useState("");
  const [requestIdError, setRequestIdError] = useState<string | null>(null);

  function handleOpenManualRequest(): void {
    try {
      const normalized = requestIdInput.trim();
      parseRequestIdHex(normalized);
      setRequestIdError(null);
      props.onOpen(normalized);
    } catch (error) {
      setRequestIdError(error instanceof Error ? error.message : "history.request_id_invalid");
    }
  }

  return (
    <Card className="rounded-2xl border-emerald-100">
      <CardHeader>
        <CardTitle>Recent Requests</CardTitle>
        <CardDescription>
          request_id を手入力で開けます。履歴は Juno 設定時のみ永続化されます。
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2 rounded-xl border border-zinc-200 bg-zinc-50/70 p-3">
          <p className="text-xs font-semibold text-zinc-600">Open by request_id</p>
          <div className="flex gap-2">
            <Input
              placeholder="0x..."
              value={requestIdInput}
              onChange={(event) => setRequestIdInput(event.target.value)}
            />
            <Button
              type="button"
              variant="outline"
              onClick={handleOpenManualRequest}
              disabled={requestIdInput.trim() === ""}
            >
              Open
            </Button>
          </div>
          {requestIdError ? (
            <p className="text-xs text-rose-700">{requestIdError}</p>
          ) : null}
          {!props.walletConnected ? (
            <p className="text-xs text-zinc-500">ウォレット未接続でも request_id を開けます。</p>
          ) : null}
          {props.error ? (
            <p className="text-xs text-zinc-500">history: {props.error}</p>
          ) : null}
        </div>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>kind</TableHead>
              <TableHead>tracking_id</TableHead>
              <TableHead className="text-right">action</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {props.loading ? (
              <TableRow>
                <TableCell colSpan={3} className="text-center text-zinc-500">
                  履歴を読み込み中...
                </TableCell>
              </TableRow>
            ) : props.history.length === 0 ? (
              <TableRow>
                <TableCell colSpan={3} className="text-center text-zinc-500">
                  履歴なし
                </TableCell>
              </TableRow>
            ) : (
              props.history.map((item) => (
                <TableRow key={`${item.requestId}:${item.submittedAt}`}>
                  <TableCell>{item.kind}</TableCell>
                  <TableCell className="font-mono text-xs">{item.requestId}</TableCell>
                  <TableCell className="text-right">
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => props.onOpen(item.requestId)}
                    >
                      Open
                    </Button>
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}
