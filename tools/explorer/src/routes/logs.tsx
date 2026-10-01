// どこで: Logsページ / 何を: topic0/address/rangeの検索結果を表示 / なぜ: 運用時のイベント調査をブラウザで完結させるため

import { createFileRoute, Link, redirect } from "@tanstack/react-router";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { LogsSearchForm } from "../../components/logs-search-form";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../components/ui/table";
import { TxHashLink } from "../../components/tx-hash-link";

import { createServerFn } from "@tanstack/react-start";
import { validatePageInput } from "../../lib/page-input";
import { parseUrlSearch, searchString } from "../../lib/url-search";

type Input = { fromBlock?: string; toBlock?: string; address?: string; topic0?: string; topic1?: string; blockHash?: string; window?: string; cursor?: string };
const loadPage = createServerFn({ method: "GET" })
  .validator((input: Input) => validatePageInput(input, []))
  .handler(async ({ data: input }) => {
    const { getLogsView } = await import("../../lib/logs");
    const data = await getLogsView(input);
    if (shouldRedirectToCanonical(input)) throw redirect({ href: `/logs?${buildCanonicalQuery(data.filters)}`, statusCode: 307 });
    return { data };
  });

export const Route = createFileRoute("/logs")({
  validateSearch: (search: Record<string, unknown>): Input => ({ fromBlock: searchString(search.fromBlock), toBlock: searchString(search.toBlock), address: searchString(search.address), topic0: searchString(search.topic0), topic1: searchString(search.topic1), blockHash: searchString(search.blockHash), window: searchString(search.window), cursor: searchString(search.cursor) }),
  loaderDeps: ({ search }) => search,
  loader: ({ deps }) => loadPage({ data: deps }),
  component: LogsPage,
});

function LogsPage() {
  const { data } = Route.useLoaderData();
  const query = new URLSearchParams();
  if (data.filters.fromBlock) query.set("fromBlock", data.filters.fromBlock);
  if (data.filters.toBlock) query.set("toBlock", data.filters.toBlock);
  if (data.filters.address) query.set("address", data.filters.address);
  if (data.filters.topic0) query.set("topic0", data.filters.topic0);
  if (data.filters.blockHash) query.set("blockHash", data.filters.blockHash);
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
                    <TableCell>{item.blockNumber}</TableCell>
                    <TableCell>{item.txIndex}</TableCell>
                    <TableCell>{item.logIndex}</TableCell>
                    <TableCell className="font-mono break-all">{item.addressHex}</TableCell>
                    <TableCell className="font-mono break-all">{item.topic0Hex ?? "-"}</TableCell>
                    <TableCell className="font-mono">
                      <TxHashLink
                        txHashHex={item.txHashHex}
                        receiptStatus={item.receiptStatus}
                        className="text-sky-700 hover:underline break-all"
                        title={item.txHashHex}
                      >
                        {item.txHashHex}
                      </TxHashLink>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
          {data.nextCursor ? (
            <div className="mt-3 text-sm">
              <Link to="/logs" search={parseUrlSearch(withCursor(query, data.nextCursor))} className="text-sky-700 hover:underline">Older</Link>
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
  blockHash: string;
  window: string;
}): URLSearchParams {
  const query = new URLSearchParams();
  if (filters.fromBlock) query.set("fromBlock", filters.fromBlock);
  if (filters.toBlock) query.set("toBlock", filters.toBlock);
  if (filters.address) query.set("address", filters.address);
  if (filters.topic0) query.set("topic0", filters.topic0);
  if (filters.blockHash) query.set("blockHash", filters.blockHash);
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
