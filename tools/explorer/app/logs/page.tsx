// どこで: Logsページ / 何を: topic0/address/rangeの検索結果を表示 / なぜ: 運用時のイベント調査をブラウザで完結させるため

import Link from "next/link";
import { redirect } from "next/navigation";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { LogsSearchForm } from "../../components/logs-search-form";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../components/ui/table";
import { getLogsView } from "../../lib/logs";

export const dynamic = "force-dynamic";

export default async function LogsPage({
  searchParams,
}: {
  searchParams: Promise<{
    fromBlock?: string;
    toBlock?: string;
    address?: string;
    topic0?: string;
    blockHash?: string;
    window?: string;
    cursor?: string;
  }>;
}) {
  const params = await searchParams;
  const data = await getLogsView(params);
  if (shouldRedirectToCanonical(params)) {
    const canonical = buildCanonicalQuery(data.filters);
    redirect(`/logs?${canonical.toString()}`);
  }
  const query = new URLSearchParams();
  if (data.filters.fromBlock) query.set("fromBlock", data.filters.fromBlock);
  if (data.filters.toBlock) query.set("toBlock", data.filters.toBlock);
  if (data.filters.address) query.set("address", data.filters.address);
  if (data.filters.topic0) query.set("topic0", data.filters.topic0);
  if (data.filters.window) query.set("window", data.filters.window);

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Logs</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <LogsSearchForm initialFilters={data.filters} />
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
                    <TableCell className="font-mono break-all">{item.addressHex}</TableCell>
                    <TableCell className="font-mono break-all">{item.topic0Hex ?? "-"}</TableCell>
                    <TableCell className="font-mono">
                      <Link href={`/tx/${item.txHashHex}`} className="text-sky-700 hover:underline break-all">{item.txHashHex}</Link>
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

function buildCanonicalQuery(filters: {
  fromBlock: string;
  toBlock: string;
  address: string;
  topic0: string;
  window: string;
}): URLSearchParams {
  const query = new URLSearchParams();
  if (filters.fromBlock) query.set("fromBlock", filters.fromBlock);
  if (filters.toBlock) query.set("toBlock", filters.toBlock);
  if (filters.address) query.set("address", filters.address);
  if (filters.topic0) query.set("topic0", filters.topic0);
  if (filters.window) query.set("window", filters.window);
  return query;
}

function shouldRedirectToCanonical(
  raw: {
    fromBlock?: string;
    toBlock?: string;
    address?: string;
    topic0?: string;
    topic1?: string;
    blockHash?: string;
    window?: string;
    cursor?: string;
  }
): boolean {
  return raw.topic1 !== undefined && raw.topic1.trim() !== "";
}
