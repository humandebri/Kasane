// どこで: ホームページ / 何を: testnetの主要情報をEtherscan風の密度で表示 / なぜ: 公開時の初動確認を素早くするため

import { createFileRoute, Link } from "@tanstack/react-router";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../components/ui/table";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { TxHashLink } from "../../components/tx-hash-link";
import { TxValueFeeCells } from "../../components/tx-value-fee-cells";
import { toHexLower } from "../../lib/hex";
import { inferMethodLabel, shortenMethodLabel } from "../../lib/tx_method";

import { createServerFn } from "@tanstack/react-start";
import { validatePageInput } from "../../lib/page-input";
import { searchStrings } from "../../lib/url-search";

type Input = { blocks?: string | string[] };
const loadPage = createServerFn({ method: "GET" })
  .validator((input: Input) => validatePageInput(input, [], ["blocks"]))
  .handler(async ({ data: input }) => {
    const { getHomeView } = await import("../../lib/data");
    const { publicRpcConfig } = await import("../../lib/public-config");
    return { renderedAtMs: Date.now(), data: await getHomeView(input.blocks), ...publicRpcConfig() };
  });

export const Route = createFileRoute("/")({
  validateSearch: (search: Record<string, unknown>): Input => ({ blocks: searchStrings(search.blocks) }),
  loaderDeps: ({ search }) => search,
  loader: ({ deps }) => loadPage({ data: deps }),
  component: HomePage,
});

function HomePage() {
  const { renderedAtMs, data, canisterId, icHost } = Route.useLoaderData();
  return (
    <>
      <section className="grid gap-4">
        <Card className="fade-in gap-4 border-slate-200 bg-white py-4 shadow-sm">
          <CardHeader className="space-y-2">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <CardTitle className="text-xl tracking-tight">Kasane Testnet Explorer</CardTitle>
              <div className="flex flex-wrap gap-2">
                <Link to="/ops" className="inline-flex">
                  <Button type="button" variant="secondary" className="rounded-full">
                    Open Ops
                  </Button>
                </Link>
                <Link to="/logs" className="inline-flex">
                  <Button type="button" variant="secondary" className="rounded-full">
                    Open Logs
                  </Button>
                </Link>
              </div>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            <form action="/search" className="flex flex-col gap-2 md:flex-row">
              <Input
                name="q"
                required
                placeholder="Search by Block / Transaction / Address / Principal"
                className="h-11 rounded-full border-slate-300 bg-white font-mono"
              />
              <Button type="submit" className="h-11 rounded-full px-5">
                Search
              </Button>
            </form>

            <div className="grid gap-2 text-sm sm:grid-cols-2">
              <div className="rounded-xl border border-slate-200 bg-slate-50/70 p-3">
                <p className="text-xs uppercase tracking-wide text-slate-500">Latest Metrics Day</p>
                <p className="mt-1 font-medium text-slate-900">{data.stats.latestDay ?? "-"}</p>
              </div>
              <div className="rounded-xl border border-slate-200 bg-slate-50/70 p-3">
                <p className="text-xs uppercase tracking-wide text-slate-500">Daily Blocks Ingested</p>
                <p className="mt-1 font-medium text-slate-900">{data.stats.latestDayBlocks.toString()}</p>
              </div>
              <div className="rounded-xl border border-slate-200 bg-slate-50/70 p-3">
                <p className="text-xs uppercase tracking-wide text-slate-500">Daily Raw Bytes</p>
                <p className="mt-1 font-medium text-slate-900">{data.stats.latestDayRawBytes.toString()}</p>
              </div>
              <div className="rounded-xl border border-slate-200 bg-slate-50/70 p-3">
                <p className="text-xs uppercase tracking-wide text-slate-500">Daily Compressed Bytes</p>
                <p className="mt-1 font-medium text-slate-900">{data.stats.latestDayCompressedBytes.toString()}</p>
              </div>
            </div>
          </CardContent>
        </Card>
      </section>

      <section className="grid gap-4 xl:grid-cols-3">
        <Card className="fade-in gap-4 border-slate-200 bg-white py-4 shadow-sm xl:col-span-1">
          <CardHeader className="flex flex-row items-center justify-between gap-3">
            <CardTitle>Latest Blocks</CardTitle>
            <Link to="/blocks" className="text-sm text-sky-700 hover:underline">
              View more
            </Link>
          </CardHeader>
          <CardContent>
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
                    <TableCell>
                      <Link to="/txs" search={{ block: block.number.toString() }} className="text-sky-700 hover:underline">
                        {block.txCount}
                      </Link>
                    </TableCell>
                    <TableCell>{formatGasUsed(block.gasUsed)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>

        <Card className="fade-in gap-4 border-slate-200 bg-white py-4 shadow-sm xl:col-span-2">
          <CardHeader className="flex flex-row items-center justify-between gap-3">
            <CardTitle>Latest Transactions</CardTitle>
            <div className="flex items-center gap-2">
              <Link to="/txs" className="text-sm text-sky-700 hover:underline">
                View more
              </Link>
            </div>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Transaction Hash</TableHead>
                  <TableHead>Method</TableHead>
                  <TableHead>Age</TableHead>
                  <TableHead>From</TableHead>
                  <TableHead>To</TableHead>
                  <TableHead>Amount</TableHead>
                  <TableHead>Txn Fee</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.txs.map((tx) => {
                  return (
                    <TableRow key={tx.txHashHex}>
                      <TableCell className="font-mono text-xs">
                        <TxHashLink txHashHex={tx.txHashHex} receiptStatus={tx.receiptStatus}>
                          {shortPrefixHex(tx.txHashHex)}
                        </TxHashLink>
                      </TableCell>
                      <TableCell className="text-xs">
                        {shortenMethodLabel(inferMethodLabel(tx.toAddress ? toHexLower(tx.toAddress) : null, tx.txSelector), 10)}
                      </TableCell>
                      <TableCell>{formatAge(tx.blockTimestamp, renderedAtMs)}</TableCell>
                      <TableCell className="font-mono text-xs">
                        <Link to="/address/$hex" params={{ hex: toHexLower(tx.fromAddress) }} className="text-sky-700 hover:underline">
                          {headTailHex(toHexLower(tx.fromAddress))}
                        </Link>
                      </TableCell>
                      <TableCell className="font-mono text-xs">
                        {tx.toAddress ? (
                          <Link to="/address/$hex" params={{ hex: toHexLower(tx.toAddress) }} className="text-sky-700 hover:underline">
                            {headTailHex(toHexLower(tx.toAddress))}
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
                  );
                })}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </section>
    </>
  );
}

function formatBlockAge(rawTimestamp: bigint, renderedAtMs: number): string {
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

function formatGasUsed(value: bigint | null): string {
  if (value === null) {
    return "N/A";
  }
  return value.toString();
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

function shortPrefixHex(value: string, keep: number = 10): string {
  if (value.length <= keep) {
    return value;
  }
  return `${value.slice(0, keep)}...`;
}

function headTailHex(value: string, head: number = 5, tail: number = 5): string {
  if (value.length <= head + tail) {
    return value;
  }
  return `${value.slice(0, head)}...${value.slice(-tail)}`;
}
