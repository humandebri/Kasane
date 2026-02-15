// どこで: Principal詳細ページ / 何を: caller_principal一致のtx一覧を表示 / なぜ: principal起点の追跡導線を提供するため

import Link from "next/link";
import { notFound } from "next/navigation";
import { Principal } from "@dfinity/principal";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../../components/ui/table";
import { getPrincipalView } from "../../../lib/data";
import { shortHex } from "../../../lib/hex";

export const dynamic = "force-dynamic";

export default async function PrincipalPage({ params }: { params: Promise<{ text: string }> }) {
  const { text } = await params;
  if (!isValidPrincipal(text)) {
    notFound();
  }
  const data = await getPrincipalView(text);
  return (
    <Card>
      <CardHeader>
        <CardTitle>Principal</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[180px_1fr]">
          <dt className="text-muted-foreground">Principal</dt>
          <dd className="font-mono break-all">{data.principalText}</dd>
          <dt className="text-muted-foreground">Matched Txs</dt>
          <dd>{data.txs.length}</dd>
        </dl>

        {data.txs.length === 0 ? (
          <p className="text-sm text-muted-foreground">No indexed transactions for this principal.</p>
        ) : (
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
                  <TableCell>
                    <Link href={`/blocks/${tx.blockNumber.toString()}`} className="text-sky-700 hover:underline">
                      {tx.blockNumber.toString()}
                    </Link>
                  </TableCell>
                  <TableCell>{tx.txIndex}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

function isValidPrincipal(value: string): boolean {
  try {
    Principal.fromText(value);
    return true;
  } catch {
    return false;
  }
}
