// どこで: dashboard history panel / 何を: 直近request履歴を表示 / なぜ: 再照会を素早く行えるようにするため

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

export function HistoryPanel(props: {
  history: HistoryEntry[];
  onQuery: (requestId: string) => void;
}): ReactElement {
  return (
    <Card className="rounded-2xl border-emerald-100">
      <CardHeader>
        <CardTitle>Recent Requests</CardTitle>
        <CardDescription>直近20件を保持します。</CardDescription>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>kind</TableHead>
              <TableHead>tracking_id</TableHead>
              <TableHead className="text-right">action</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {props.history.length === 0 ? (
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
                      onClick={() => props.onQuery(item.requestId)}
                    >
                      Query
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
