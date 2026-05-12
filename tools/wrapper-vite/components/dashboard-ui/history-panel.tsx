// どこで: dashboard history panel / 何を: Oisy principal 単位の recent requests を表示 / なぜ: MetaMask unwrap と履歴保存の境界を画面で明示するため

import { Button } from "@/components/ui/button";
import type { ReactElement } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { HistoryEntry } from "./types";

const DISCONNECTED_MESSAGE = "Connect Oisy to view request history.";

export function HistoryPanel(props: {
  history: HistoryEntry[];
  loading: boolean;
  error: string | null;
  walletConnected: boolean;
  onOpen: (requestId: string) => void;
}): ReactElement {
  return (
    <Card
      data-testid="history-panel"
      className="flex min-h-[22rem] flex-col rounded-2xl border-zinc-200"
    >
      <CardHeader>
        <CardTitle>Recent Requests</CardTitle>
        <CardDescription>Shows the 20 most recent request IDs submitted through signer-backed flows.</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-1 flex-col">
        {!props.walletConnected ? (
          <p className="text-sm text-zinc-500">{DISCONNECTED_MESSAGE}</p>
        ) : null}
        {props.walletConnected && props.loading ? (
          <p className="mb-3 text-sm text-zinc-500">Loading history...</p>
        ) : null}
        {props.walletConnected && props.error ? (
          <p className="mb-3 text-sm text-rose-700">history error: {props.error}</p>
        ) : null}
        <div className="mt-3 flex-1 overflow-auto">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>kind</TableHead>
                <TableHead>tracking_id</TableHead>
                <TableHead className="text-right">action</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {!props.walletConnected ? (
                <TableRow>
                  <TableCell colSpan={3} className="text-center text-zinc-500">
                    {DISCONNECTED_MESSAGE}
                  </TableCell>
                </TableRow>
              ) : props.history.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={3} className="text-center text-zinc-500">
                    No history yet
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
        </div>
      </CardContent>
    </Card>
  );
}
