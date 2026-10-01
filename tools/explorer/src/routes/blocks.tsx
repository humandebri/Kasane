// どこで: Blocks一覧ページ / 何を: 最新ブロックをページング表示 / なぜ: Homeを軽く保ちながら一覧導線を提供するため

import { createFileRoute, Link } from "@tanstack/react-router";
import { Button } from "../../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../components/ui/table";

import { createServerFn } from "@tanstack/react-start";
import { validatePageInput } from "../../lib/page-input";
import { searchStrings } from "../../lib/url-search";

type Input = { page?: string | string[]; limit?: string | string[] };
const loadPage = createServerFn({ method: "GET" })
  .validator((input: Input) => validatePageInput(input, [], ["page", "limit"]))
  .handler(async ({ data: input }) => {
    const { getLatestBlocksPageView } = await import("../../lib/data");
    return { renderedAtMs: Date.now(), data: await getLatestBlocksPageView(input.page, input.limit) };
  });

export const Route = createFileRoute("/blocks")({
  validateSearch: (search: Record<string, unknown>): Input => ({ page: searchStrings(search.page), limit: searchStrings(search.limit) }),
  loaderDeps: ({ search }) => search,
  loader: ({ deps }) => loadPage({ data: deps }),
  component: BlocksPage,
});

function BlocksPage() {
  const { renderedAtMs, data } = Route.useLoaderData();
  const prevHref = { to: "/blocks" as const, search: { page: String(data.page - 1), limit: String(data.limit) } };
  const nextHref = { to: "/blocks" as const, search: { page: String(data.page + 1), limit: String(data.limit) } };
  return (
    <Card className="border-slate-200 bg-white shadow-sm">
      <CardHeader className="flex flex-row items-center justify-between gap-3">
        <CardTitle>Latest Blocks</CardTitle>
        <div className="text-sm text-slate-600">
          page {data.page} / limit {data.limit}
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Block</TableHead>
              <TableHead>Age</TableHead>
              <TableHead>Txn</TableHead>
              <TableHead>Gas Used</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {data.blocks.map((block) => (
              <TableRow key={block.number.toString()}>
                <TableCell>
                  <Link to="/blocks/$number" params={{ number: block.number.toString() }} className="text-sky-700 hover:underline">
                    {block.number.toString()}
                  </Link>
                </TableCell>
                <TableCell>{formatBlockAge(block.timestamp, renderedAtMs)}</TableCell>
                <TableCell>{block.txCount}</TableCell>
                <TableCell>{formatGasUsed(block.gasUsed)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>

        <div className="flex flex-wrap gap-2">
          {data.hasPrev ? (
            <Link {...prevHref} className="inline-flex">
              <Button type="button" variant="secondary" className="rounded-full">
                Newer
              </Button>
            </Link>
          ) : null}
          {data.hasNext ? (
            <Link {...nextHref} className="inline-flex">
              <Button type="button" variant="secondary" className="rounded-full">
                Older
              </Button>
            </Link>
          ) : null}
        </div>
      </CardContent>
    </Card>
  );
}

function formatBlockAge(rawTimestamp: bigint, renderedAtMs: number): string {
  const nowSec = BigInt(Math.floor(renderedAtMs / 1000));
  const tsSec = rawTimestamp > 10_000_000_000n ? rawTimestamp / 1000n : rawTimestamp;
  const delta = nowSec > tsSec ? nowSec - tsSec : 0n;
  if (delta < 60n) return `${delta.toString()}s ago`;
  if (delta < 3600n) return `${(delta / 60n).toString()}m ago`;
  if (delta < 86_400n) return `${(delta / 3600n).toString()}h ago`;
  return `${(delta / 86_400n).toString()}d ago`;
}

function formatGasUsed(value: bigint | null): string {
  return value === null ? "N/A" : value.toString();
}
