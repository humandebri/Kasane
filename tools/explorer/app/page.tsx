// どこで: ホームページ / 何を: testnetの主要情報をEtherscan風の密度で表示 / なぜ: 公開時の初動確認を素早くするため

import Link from "next/link";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../components/ui/table";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { TxHashLink } from "../components/tx-hash-link";
import { TxValueFeeCells } from "../components/tx-value-fee-cells";
import { getHomeView } from "../lib/data";
import { toHexLower } from "../lib/hex";
import { inferMethodLabel, shortenMethodLabel } from "../lib/tx_method";

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
  const canisterId = process.env.EVM_CANISTER_ID ?? null;
  const icHost = process.env.EXPLORER_IC_HOST ?? process.env.INDEXER_IC_HOST ?? "https://icp-api.io";

  return (
    <>
      <section className="grid gap-4">
        <Card className="fade-in gap-4 border-slate-200 bg-white py-4 shadow-sm">
          <CardHeader className="space-y-2">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <CardTitle className="text-xl tracking-tight">Kasane Testnet Explorer</CardTitle>
              <div className="flex flex-wrap gap-2">
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
            <Link href="/blocks" className="text-sm text-sky-700 hover:underline">
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
                      <Link href={`/blocks/${block.number.toString()}`} className="text-sky-700 hover:underline">
                        {block.number.toString()}
                      </Link>
                    </TableCell>
                    <TableCell>{formatBlockAge(block.timestamp)}</TableCell>
                    <TableCell>
                      <Link href={`/txs?block=${block.number.toString()}`} className="text-sky-700 hover:underline">
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
              <Link href="/txs" className="text-sm text-sky-700 hover:underline">
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
                      <TableCell>{formatAge(tx.blockTimestamp)}</TableCell>
                      <TableCell className="font-mono text-xs">
                        <Link href={`/address/${toHexLower(tx.fromAddress)}`} className="text-sky-700 hover:underline">
                          {headTailHex(toHexLower(tx.fromAddress))}
                        </Link>
                      </TableCell>
                      <TableCell className="font-mono text-xs">
                        {tx.toAddress ? (
                          <Link href={`/address/${toHexLower(tx.toAddress)}`} className="text-sky-700 hover:underline">
                            {headTailHex(toHexLower(tx.toAddress))}
                          </Link>
                        ) : tx.createdContractAddress ? (
                          <Link href={`/address/${toHexLower(tx.createdContractAddress)}`} className="text-sky-700 hover:underline">
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

function formatBlockAge(rawTimestamp: bigint): string {
  const nowSec = BigInt(Math.floor(Date.now() / 1000));
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

function formatAge(rawTimestamp: bigint | null): string {
  if (rawTimestamp === null) {
    return "N/A";
  }
  const nowSec = BigInt(Math.floor(Date.now() / 1000));
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
