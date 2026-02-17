// どこで: Logsページ / 何を: topic0/address/rangeの検索結果を表示 / なぜ: 運用時のイベント調査をブラウザで完結させるため

import Link from "next/link";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../components/ui/table";
import { getLogsView } from "../../lib/logs";
import { shortHex } from "../../lib/hex";

export const dynamic = "force-dynamic";

export default async function LogsPage({
  searchParams,
}: {
  searchParams: Promise<{
    fromBlock?: string;
    toBlock?: string;
    address?: string;
    topic0?: string;
    topic1?: string;
    blockHash?: string;
    limit?: string;
    cursor?: string;
  }>;
}) {
  const params = await searchParams;
  const data = await getLogsView(params);
  const query = new URLSearchParams();
  if (data.filters.fromBlock) query.set("fromBlock", data.filters.fromBlock);
  if (data.filters.toBlock) query.set("toBlock", data.filters.toBlock);
  if (data.filters.address) query.set("address", data.filters.address);
  if (data.filters.topic0) query.set("topic0", data.filters.topic0);
  if (data.filters.limit) query.set("limit", data.filters.limit);

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Logs</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <form action="/logs" className="grid grid-cols-1 gap-2 md:grid-cols-6">
            <input name="fromBlock" placeholder="fromBlock" defaultValue={data.filters.fromBlock} className="h-9 rounded-md border px-3 text-sm" />
            <input name="toBlock" placeholder="toBlock" defaultValue={data.filters.toBlock} className="h-9 rounded-md border px-3 text-sm" />
            <input name="address" placeholder="address" defaultValue={data.filters.address} className="h-9 rounded-md border px-3 text-sm font-mono" />
            <input name="topic0" placeholder="topic0" defaultValue={data.filters.topic0} className="h-9 rounded-md border px-3 text-sm font-mono" />
            <input name="limit" placeholder="limit" defaultValue={data.filters.limit} className="h-9 rounded-md border px-3 text-sm" />
            <button type="submit" className="h-9 rounded-md border px-3 text-sm">Search</button>
          </form>
          <div className="rounded-md border bg-amber-50 p-3 text-sm">
            <div className="font-medium">未対応/制限</div>
            <ul className="list-disc pl-5">
              {data.unsupportedNotes.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          </div>
          {data.error ? <div className="rounded-md border bg-rose-50 p-3 text-sm">{data.error}</div> : null}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Results</CardTitle>
        </CardHeader>
        <CardContent>
          {data.items.length === 0 ? (
            <p className="text-sm text-muted-foreground">No logs.</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Block</TableHead>
                  <TableHead>Tx</TableHead>
                  <TableHead>Log</TableHead>
                  <TableHead>Address</TableHead>
                  <TableHead>topic0</TableHead>
                  <TableHead>Tx Hash</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.items.map((item) => (
                  <TableRow key={`${item.txHashHex}:${item.logIndex}`}>
                    <TableCell>{item.blockNumber.toString()}</TableCell>
                    <TableCell>{item.txIndex}</TableCell>
                    <TableCell>{item.logIndex}</TableCell>
                    <TableCell className="font-mono">{shortHex(item.addressHex)}</TableCell>
                    <TableCell className="font-mono">{item.topic0Hex ? shortHex(item.topic0Hex) : "-"}</TableCell>
                    <TableCell className="font-mono">
                      <Link href={`/tx/${item.txHashHex}`} className="text-sky-700 hover:underline">{shortHex(item.txHashHex)}</Link>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
          {data.nextCursor ? (
            <div className="mt-3 text-sm">
              <Link href={`/logs?${withCursor(query, data.nextCursor)}`} className="text-sky-700 hover:underline">Older</Link>
            </div>
          ) : null}
        </CardContent>
      </Card>
    </>
  );
}

function withCursor(query: URLSearchParams, cursor: string): string {
  const q = new URLSearchParams(query);
  q.set("cursor", cursor);
  return q.toString();
}
