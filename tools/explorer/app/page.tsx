// どこで: ホームページ / 何を: testnetの主要情報をEtherscan風の密度で表示 / なぜ: 公開時の初動確認を素早くするため

import Link from "next/link";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../components/ui/table";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { getHomeView } from "../lib/data";
import { formatTimestampUtc } from "../lib/format";
import { shortHex } from "../lib/hex";

export const dynamic = "force-dynamic";

export default async function HomePage({
  searchParams,
}: {
  searchParams: Promise<{
    blocks?: string | string[];
  }>;
}) {
  const params = await searchParams;
  const data = await getHomeView(params.blocks);

  return (
    <>
      <section className="grid gap-4">
        <Card className="fade-in border-slate-200 bg-white shadow-sm">
          <CardHeader className="space-y-2">
            <CardTitle className="text-xl tracking-tight">Kasane Testnet Explorer</CardTitle>
            <p className="text-sm text-slate-600">Search + Monitoring</p>
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

      <section className="flex flex-wrap gap-2">
        <Link href="/ops" className="inline-flex">
          <Button type="button" variant="secondary" className="rounded-full">
            Open Ops
          </Button>
        </Link>
        <Link href="/logs" className="inline-flex">
          <Button type="button" variant="secondary" className="rounded-full">
            Open Logs
          </Button>
        </Link>
      </section>

      <section className="grid gap-4 xl:grid-cols-2">
        <Card className="fade-in border-slate-200 bg-white shadow-sm">
          <CardHeader>
            <CardTitle>Latest Blocks</CardTitle>
            <p className="text-sm text-slate-600">Showing latest {data.blockLimit} blocks</p>
          </CardHeader>
          <CardContent>
            <form action="/" className="mb-3 flex flex-wrap items-center gap-2">
              <label htmlFor="blocks" className="text-sm text-slate-600">
                Count
              </label>
              <Input
                id="blocks"
                name="blocks"
                type="number"
                min={1}
                max={500}
                defaultValue={data.blockLimit.toString()}
                className="h-9 w-28 border-slate-300 bg-white text-sm"
              />
              <Button type="submit" variant="secondary" className="h-9 px-3">
                Update
              </Button>
            </form>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Block</TableHead>
                  <TableHead>Hash</TableHead>
                  <TableHead>Timestamp</TableHead>
                  <TableHead>Txs</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.blocks.map((block) => (
                  <TableRow key={block.number.toString()}>
                    <TableCell>
                      <Link href={`/blocks/${block.number.toString()}`} className="text-sky-700 hover:underline">
                        {block.number.toString()}
                      </Link>
                    </TableCell>
                    <TableCell className="font-mono text-xs">{block.hashHex ? shortHex(block.hashHex) : "-"}</TableCell>
                    <TableCell>{formatTimestampUtc(block.timestamp)}</TableCell>
                    <TableCell>{block.txCount}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>

        <Card className="fade-in border-slate-200 bg-white shadow-sm">
          <CardHeader>
            <CardTitle>Latest Transactions</CardTitle>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Tx Hash</TableHead>
                  <TableHead>Block</TableHead>
                  <TableHead>Index</TableHead>
                  <TableHead>Principal</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.txs.map((tx) => (
                  <TableRow key={tx.txHashHex}>
                    <TableCell className="font-mono text-xs">
                      <Link href={`/tx/${tx.txHashHex}`} className="text-sky-700 hover:underline">
                        {shortHex(tx.txHashHex)}
                      </Link>
                    </TableCell>
                    <TableCell>{tx.blockNumber.toString()}</TableCell>
                    <TableCell>{tx.txIndex}</TableCell>
                    <TableCell className="font-mono text-xs">
                      {tx.callerPrincipalText ? (
                        <Link href={`/principal/${encodeURIComponent(tx.callerPrincipalText)}`} className="text-sky-700 hover:underline">
                          {tx.callerPrincipalText}
                        </Link>
                      ) : (
                        "-"
                      )}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </section>
    </>
  );
}
