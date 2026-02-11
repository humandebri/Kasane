// どこで: ホームページ / 何を: head・最新blocks・最新txを表示 / なぜ: 運用時の初動確認を1画面に集約するため

import Link from "next/link";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../components/ui/table";
import { Badge } from "../components/ui/badge";
import { getHomeView } from "../lib/data";
import { shortHex } from "../lib/hex";

export const dynamic = "force-dynamic";

export default async function HomePage() {
  const data = await getHomeView();
  const lag = data.dbHead === null ? "N/A" : (data.rpcHead - data.dbHead).toString();

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Head</CardTitle>
        </CardHeader>
        <CardContent>
          <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[180px_1fr]">
            <dt className="text-muted-foreground">RPC Head</dt>
            <dd>{data.rpcHead.toString()}</dd>
            <dt className="text-muted-foreground">DB Head</dt>
            <dd>{data.dbHead ? data.dbHead.toString() : "(no blocks)"}</dd>
            <dt className="text-muted-foreground">Lag</dt>
            <dd>
              <Badge variant="outline">{lag}</Badge>
            </dd>
          </dl>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Operations</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <form action="/search" className="flex flex-col gap-2 md:flex-row">
            <input
              name="q"
              placeholder="block number or tx hash"
              className="h-9 rounded-md border px-3 font-mono text-sm"
              required
            />
            <button type="submit" className="h-9 rounded-md border px-3 text-sm">
              Search
            </button>
          </form>
          <dl className="grid grid-cols-1 gap-2 md:grid-cols-[180px_1fr]">
            <dt className="text-muted-foreground">Total Blocks</dt>
            <dd>{data.stats.totalBlocks.toString()}</dd>
            <dt className="text-muted-foreground">Total Txs</dt>
            <dd>{data.stats.totalTxs.toString()}</dd>
            <dt className="text-muted-foreground">Latest Metrics Day</dt>
            <dd>{data.stats.latestDay ?? "-"}</dd>
            <dt className="text-muted-foreground">Blocks Ingested (Day)</dt>
            <dd>{data.stats.latestDayBlocks.toString()}</dd>
            <dt className="text-muted-foreground">Raw Bytes (Day)</dt>
            <dd>{data.stats.latestDayRawBytes.toString()}</dd>
            <dt className="text-muted-foreground">Compressed Bytes (Day)</dt>
            <dd>{data.stats.latestDayCompressedBytes.toString()}</dd>
          </dl>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Latest Blocks</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Number</TableHead>
                <TableHead>Hash</TableHead>
                <TableHead>Timestamp</TableHead>
                <TableHead>Tx Count</TableHead>
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
                  <TableCell className="font-mono">{block.hashHex ? shortHex(block.hashHex) : "-"}</TableCell>
                  <TableCell>{block.timestamp.toString()}</TableCell>
                  <TableCell>{block.txCount}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Latest Txs</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Tx Hash</TableHead>
                <TableHead>Block</TableHead>
                <TableHead>Index</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data.txs.map((tx) => (
                <TableRow key={tx.txHashHex}>
                  <TableCell className="font-mono">
                    <Link href={`/tx/${tx.txHashHex}`} className="text-sky-700 hover:underline">
                      {shortHex(tx.txHashHex)}
                    </Link>
                  </TableCell>
                  <TableCell>{tx.blockNumber.toString()}</TableCell>
                  <TableCell>{tx.txIndex}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </>
  );
}
