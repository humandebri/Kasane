// どこで: Tx一覧ページ / 何を: 最新トランザクションをページング表示 / なぜ: Homeの20件を超える閲覧導線を提供するため

import { createFileRoute, Link } from "@tanstack/react-router";
import { Button } from "../../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../components/ui/table";
import { TxHashLink } from "../../components/tx-hash-link";
import { TxValueFeeCells } from "../../components/tx-value-fee-cells";
import { shortHex, toHexLower } from "../../lib/hex";
import { inferMethodLabel, shortenMethodLabel } from "../../lib/tx_method";

import { createServerFn } from "@tanstack/react-start";
import { validatePageInput } from "../../lib/page-input";
import { parseUrlSearch, searchStrings } from "../../lib/url-search";

type Input = { page?: string | string[]; limit?: string | string[]; block?: string | string[] };
const loadPage = createServerFn({ method: "GET" })
  .validator((input: Input) => validatePageInput(input, [], ["page", "limit", "block"]))
  .handler(async ({ data: input }) => {
    const { getLatestTxsPageView } = await import("../../lib/data");
    const { publicRpcConfig } = await import("../../lib/public-config");
    return { renderedAtMs: Date.now(), data: await getLatestTxsPageView(input.page, input.limit, input.block), ...publicRpcConfig() };
  });

export const Route = createFileRoute("/txs")({
  validateSearch: (search: Record<string, unknown>): Input => ({ page: searchStrings(search.page), limit: searchStrings(search.limit), block: searchStrings(search.block) }),
  loaderDeps: ({ search }) => search,
  loader: ({ deps }) => loadPage({ data: deps }),
  component: LatestTxsPage,
});

function LatestTxsPage() {
  const { renderedAtMs, data, canisterId, icHost } = Route.useLoaderData();
  const firstHref = buildTxsHref(1, data.limit, data.blockNumberFilter);
  const prevHref = buildTxsHref(data.page - 1, data.limit, data.blockNumberFilter);
  const nextHref = buildTxsHref(data.page + 1, data.limit, data.blockNumberFilter);
  const lastHref = buildTxsHref(data.totalPages, data.limit, data.blockNumberFilter);

  return (
    <Card className="border-slate-200 bg-white shadow-sm py-4">
      <CardHeader className="flex flex-row items-center justify-between gap-3">
        <CardTitle>{data.blockNumberFilter === null ? "Latest Transactions" : `Transactions in Block ${data.blockNumberFilter.toString()}`}</CardTitle>
        <div className="flex flex-wrap items-center gap-2">
          {data.hasPrev ? (
            <Link {...firstHref} className="inline-flex">
              <Button type="button" variant="secondary" className="rounded-sm">
                First
              </Button>
            </Link>
          ) : (
            <Button type="button" variant="secondary" className="rounded-sm" disabled>
              First
            </Button>
          )}
          {data.hasPrev ? (
            <Link {...prevHref} className="inline-flex">
              <Button type="button" variant="secondary" className="rounded-sm">
                {"<"}
              </Button>
            </Link>
          ) : (
            <Button type="button" variant="secondary" className="rounded-sm" disabled>
              {"<"}
            </Button>
          )}
          <Button type="button" variant="secondary" className="rounded-sm" disabled>
            {`Page ${data.page} of ${data.totalPages}`}
          </Button>
          <Button type="button" variant="secondary" className="rounded-sm" disabled>
            {`Showing ${data.txs.length} / ${data.totalTxs.toString()} txs (limit ${data.limit})`}
          </Button>
          {data.hasNext ? (
            <Link {...nextHref} className="inline-flex">
              <Button type="button" variant="secondary" className="rounded-sm">
                {">"}
              </Button>
            </Link>
          ) : (
            <Button type="button" variant="secondary" className="rounded-sm" disabled>
              {">"}
            </Button>
          )}
          {data.hasNext ? (
            <Link {...lastHref} className="inline-flex">
              <Button type="button" variant="secondary" className="rounded-sm">
                Last
              </Button>
            </Link>
          ) : (
            <Button type="button" variant="secondary" className="rounded-sm" disabled>
              Last
            </Button>
          )}
        </div>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Transaction Hash</TableHead>
              <TableHead>Method</TableHead>
              <TableHead>Block</TableHead>
              <TableHead>Age</TableHead>
              <TableHead>From</TableHead>
              <TableHead>To</TableHead>
              <TableHead>Amount</TableHead>
              <TableHead>Txn Fee</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {data.txs.map((tx) => (
                <TableRow key={tx.txHashHex}>
                <TableCell className="font-mono text-xs">
                  <TxHashLink txHashHex={tx.txHashHex} receiptStatus={tx.receiptStatus}>
                    {shortHex(tx.txHashHex)}
                  </TxHashLink>
                </TableCell>
                <TableCell className="text-xs">
                  {shortenMethodLabel(inferMethodLabel(tx.toAddress ? toHexLower(tx.toAddress) : null, tx.txSelector), 10)}
                </TableCell>
                <TableCell>
                  <Link to="/blocks/$number" params={{ number: tx.blockNumber.toString() }} className="text-sky-700 hover:underline">
                    {tx.blockNumber.toString()}
                  </Link>
                </TableCell>
                <TableCell>
                  {formatAge(tx.blockTimestamp, renderedAtMs)}
                </TableCell>
                <TableCell className="font-mono text-xs">
                  <Link to="/address/$hex" params={{ hex: toHexLower(tx.fromAddress) }} className="text-sky-700 hover:underline">
                    {shortHex(toHexLower(tx.fromAddress))}
                  </Link>
                </TableCell>
                <TableCell className="font-mono text-xs">
                  {tx.toAddress ? (
                    <Link to="/address/$hex" params={{ hex: toHexLower(tx.toAddress) }} className="text-sky-700 hover:underline">
                      {shortHex(toHexLower(tx.toAddress))}
                    </Link>
                  ) : tx.createdContractAddress ? (
                    <Link to="/address/$hex" params={{ hex: toHexLower(tx.createdContractAddress) }} className="text-sky-700 hover:underline">
                      Contract Creation
                    </Link>
                  ) : (
                    "Contract Creation"
                  )}
                </TableCell>
                <TxValueFeeCells txHashHex={tx.txHashHex} canisterId={canisterId} icHost={icHost} />
              </TableRow>
            ))}
          </TableBody>
        </Table>

      </CardContent>
    </Card>
  );
}

function buildTxsHref(page: number, limit: number, blockNumberFilter: bigint | null) {
  const query = new URLSearchParams();
  query.set("page", page.toString());
  query.set("limit", limit.toString());
  if (blockNumberFilter !== null) {
    query.set("block", blockNumberFilter.toString());
  }
  return { to: "/txs" as const, search: parseUrlSearch(query.toString()) };
}

function formatAge(rawTimestamp: bigint | null, renderedAtMs: number): string {
  if (rawTimestamp === null) {
    return "N/A";
  }
  const nowSec = BigInt(Math.floor(renderedAtMs / 1000));
  const tsSec = rawTimestamp > 10_000_000_000n ? rawTimestamp / 1000n : rawTimestamp;
  const delta = nowSec > tsSec ? nowSec - tsSec : 0n;
  if (delta < 60n) {
    return `${delta.toString()}s ago`;
  }
  if (delta < 3600n) {
    return `${(delta / 60n).toString()}m ago`;
  }
  if (delta < 86_400n) {
    return `${(delta / 3600n).toString()}h ago`;
  }
  return `${(delta / 86_400n).toString()}d ago`;
}
