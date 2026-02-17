// どこで: Tx一覧ページ / 何を: 最新トランザクションをページング表示 / なぜ: Homeの20件を超える閲覧導線を提供するため

import Link from "next/link";
import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../components/ui/table";
import { TxValueFeeCells } from "../../components/tx-value-fee-cells";
import { getLatestTxsPageView } from "../../lib/data";
import { shortHex, toHexLower } from "../../lib/hex";

export const dynamic = "force-dynamic";

export default async function LatestTxsPage({
  searchParams,
}: {
  searchParams: Promise<{ page?: string | string[]; limit?: string | string[] }>;
}) {
  const params = await searchParams;
  const data = await getLatestTxsPageView(params.page, params.limit);
  const canisterId = process.env.EVM_CANISTER_ID ?? null;
  const icHost = process.env.EXPLORER_IC_HOST ?? process.env.INDEXER_IC_HOST ?? "https://icp-api.io";
  const prevHref = `/txs?page=${data.page - 1}&limit=${data.limit}`;
  const nextHref = `/txs?page=${data.page + 1}&limit=${data.limit}`;

  return (
    <Card className="border-slate-200 bg-white shadow-sm">
      <CardHeader className="flex flex-row items-center justify-between gap-3">
        <CardTitle>Latest Transactions</CardTitle>
        <div className="text-sm text-slate-600">
          page {data.page} / limit {data.limit}
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Tx Hash</TableHead>
              <TableHead>Block</TableHead>
              <TableHead>Amount</TableHead>
              <TableHead>Txn Fee</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>From</TableHead>
              <TableHead>To</TableHead>
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
                <TableCell>
                  <Link href={`/blocks/${tx.blockNumber.toString()}`} className="text-sky-700 hover:underline">
                    {tx.blockNumber.toString()}
                  </Link>
                </TableCell>
                <TxValueFeeCells txHashHex={tx.txHashHex} canisterId={canisterId} icHost={icHost} />
                <TableCell>
                  <Badge variant={tx.receiptStatus === 1 ? "secondary" : tx.receiptStatus === 0 ? "default" : "outline"}>
                    {tx.receiptStatus === 1 ? "success" : tx.receiptStatus === 0 ? "failed" : "pending"}
                  </Badge>
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
                  ) : (
                    "(create)"
                  )}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>

        <div className="flex flex-wrap gap-2">
          {data.hasPrev ? (
            <Link href={prevHref} className="inline-flex">
              <Button type="button" variant="secondary" className="rounded-full">
                Newer
              </Button>
            </Link>
          ) : null}
          {data.hasNext ? (
            <Link href={nextHref} className="inline-flex">
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
