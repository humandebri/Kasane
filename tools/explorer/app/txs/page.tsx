// どこで: Tx一覧ページ / 何を: 最新トランザクションをページング表示 / なぜ: Homeの20件を超える閲覧導線を提供するため

import Link from "next/link";
import { Button } from "../../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../components/ui/table";
import { TxValueFeeCells } from "../../components/tx-value-fee-cells";
import { getLatestTxsPageView } from "../../lib/data";
import { shortHex, toHexLower } from "../../lib/hex";
import { inferMethodLabel, shortenMethodLabel } from "../../lib/tx_method";

export const dynamic = "force-dynamic";

export default async function LatestTxsPage({
  searchParams,
}: {
  searchParams: Promise<{ page?: string | string[]; limit?: string | string[]; block?: string | string[] }>;
}) {
  const params = await searchParams;
  const data = await getLatestTxsPageView(params.page, params.limit, params.block);
  const canisterId = process.env.EVM_CANISTER_ID ?? null;
  const icHost = process.env.EXPLORER_IC_HOST ?? process.env.INDEXER_IC_HOST ?? "https://icp-api.io";
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
            <Link href={firstHref} className="inline-flex">
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
            <Link href={prevHref} className="inline-flex">
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
          {data.hasNext ? (
            <Link href={nextHref} className="inline-flex">
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
            <Link href={lastHref} className="inline-flex">
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
                  <Link href={`/tx/${tx.txHashHex}`} className="text-sky-700 hover:underline">
                    {shortHex(tx.txHashHex)}
                  </Link>
                </TableCell>
                <TableCell className="text-xs">
                  {shortenMethodLabel(inferMethodLabel(tx.toAddress ? toHexLower(tx.toAddress) : null, tx.txSelector), 10)}
                </TableCell>
                <TableCell>
                  <Link href={`/blocks/${tx.blockNumber.toString()}`} className="text-sky-700 hover:underline">
                    {tx.blockNumber.toString()}
                  </Link>
                </TableCell>
                <TableCell>
                  {formatAge(tx.blockTimestamp)}
                </TableCell>
                <TableCell className="font-mono text-xs">
                  <Link href={`/address/${toHexLower(tx.fromAddress)}`} className="text-sky-700 hover:underline">
                    {shortHex(toHexLower(tx.fromAddress))}
                  </Link>
                </TableCell>
                <TableCell className="font-mono text-xs">
                  {tx.toAddress ? (
                    <Link href={`/address/${toHexLower(tx.toAddress)}`} className="text-sky-700 hover:underline">
                      {shortHex(toHexLower(tx.toAddress))}
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
            ))}
          </TableBody>
        </Table>

      </CardContent>
    </Card>
  );
}

function buildTxsHref(page: number, limit: number, blockNumberFilter: bigint | null): string {
  const query = new URLSearchParams();
  query.set("page", page.toString());
  query.set("limit", limit.toString());
  if (blockNumberFilter !== null) {
    query.set("block", blockNumberFilter.toString());
  }
  return `/txs?${query.toString()}`;
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
