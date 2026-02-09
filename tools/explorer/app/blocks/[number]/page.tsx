// どこで: ブロック詳細ページ / 何を: block単位のindex情報を表示 / なぜ: 障害調査時に tx の収容状態を即確認するため

import Link from "next/link";
import { notFound } from "next/navigation";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../../components/ui/table";
import { getBlockView } from "../../../lib/data";
import { shortHex } from "../../../lib/hex";

export const dynamic = "force-dynamic";

function parseBlockNumber(input: string): bigint {
  if (!/^[0-9]+$/.test(input)) {
    throw new Error("block number must be decimal");
  }
  return BigInt(input);
}

export default async function BlockPage({ params }: { params: Promise<{ number: string }> }) {
  const { number } = await params;
  const blockNumber = parseBlockNumber(number);
  const data = await getBlockView(blockNumber);

  if (!data.db) {
    notFound();
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Block {data.db.block.number.toString()}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[180px_1fr]">
          <dt className="text-muted-foreground">Hash</dt>
          <dd className="font-mono">{data.db.block.hashHex ?? "-"}</dd>
          <dt className="text-muted-foreground">Timestamp</dt>
          <dd>{data.db.block.timestamp.toString()}</dd>
          <dt className="text-muted-foreground">Tx Count (DB)</dt>
          <dd>{data.db.block.txCount}</dd>
          <dt className="text-muted-foreground">Found in RPC</dt>
          <dd>
            <Badge variant={data.rpcExists ? "secondary" : "outline"}>{data.rpcExists ? "yes" : "no"}</Badge>
          </dd>
        </dl>

        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Tx Hash</TableHead>
              <TableHead>Index</TableHead>
              <TableHead>Receipt</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {data.db.txs.map((tx) => (
              <TableRow key={tx.txHashHex}>
                <TableCell className="font-mono">
                  <Link href={`/tx/${tx.txHashHex}`} className="text-sky-700 hover:underline">
                    {shortHex(tx.txHashHex)}
                  </Link>
                </TableCell>
                <TableCell>{tx.txIndex}</TableCell>
                <TableCell>
                  <Link href={`/receipt/${tx.txHashHex}`} className="text-sky-700 hover:underline">
                    open
                  </Link>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}
