// どこで: アドレス詳細ページ / 何を: アドレスのスナップショット情報を表示 / なぜ: 公開導線として残高/nonce/コード有無を即確認できるようにするため

import Link from "next/link";
import { notFound } from "next/navigation";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../../components/ui/table";
import { getAddressView } from "../../../lib/data";
import { isAddressHex, normalizeHex, shortHex } from "../../../lib/hex";

export const dynamic = "force-dynamic";

export default async function AddressPage({
  params,
  searchParams,
}: {
  params: Promise<{ hex: string }>;
  searchParams: Promise<{ cursor?: string; principal?: string }>;
}) {
  const { hex } = await params;
  const { cursor, principal } = await searchParams;
  if (!isAddressHex(hex)) {
    notFound();
  }
  const normalizedHex = normalizeHex(hex);
  const data = await getAddressView(normalizedHex, cursor);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Address</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {principal ? (
          <div className="rounded-md border bg-sky-50 p-3 text-sm">
            derived from principal:{" "}
            <Link href={`/principal/${encodeURIComponent(principal)}`} className="font-mono text-sky-700 hover:underline">
              {principal}
            </Link>
          </div>
        ) : null}
        <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[180px_1fr]">
          <dt className="text-muted-foreground">Address</dt>
          <dd className="font-mono">{data.addressHex}</dd>
          <dt className="text-muted-foreground">Balance (wei)</dt>
          <dd className="font-mono">{data.balance === null ? "N/A" : data.balance.toString()}</dd>
          <dt className="text-muted-foreground">Nonce</dt>
          <dd>{data.nonce === null ? "N/A" : data.nonce.toString()}</dd>
          <dt className="text-muted-foreground">Code Bytes</dt>
          <dd>{data.codeBytes === null ? "N/A" : data.codeBytes.toString()}</dd>
          <dt className="text-muted-foreground">Type</dt>
          <dd>
            <Badge variant={data.isContract === true ? "secondary" : "outline"}>
              {data.isContract === null ? "Unknown" : data.isContract ? "Contract" : "EOA"}
            </Badge>
          </dd>
        </dl>

        <div className="space-y-2">
          <div className="text-sm font-medium">History</div>
          {data.history.length === 0 ? (
            <div className="rounded-md border bg-slate-50 p-3 text-sm text-muted-foreground">
              No indexed transactions for this address.
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Tx Hash</TableHead>
                  <TableHead>Block</TableHead>
                  <TableHead>Index</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Direction</TableHead>
                  <TableHead>Counterparty</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.history.map((item) => (
                  <TableRow key={item.txHashHex}>
                    <TableCell className="font-mono">
                      <Link href={`/tx/${item.txHashHex}`} className="text-sky-700 hover:underline">
                        {shortHex(item.txHashHex)}
                      </Link>
                    </TableCell>
                    <TableCell>
                      <Link href={`/blocks/${item.blockNumber.toString()}`} className="text-sky-700 hover:underline">
                        {item.blockNumber.toString()}
                      </Link>
                    </TableCell>
                    <TableCell>{item.txIndex}</TableCell>
                    <TableCell>
                      {item.receiptStatus === null ? "-" : item.receiptStatus === 1 ? "success" : "failed"}
                    </TableCell>
                    <TableCell>
                      <Badge variant={item.direction === "out" ? "outline" : "secondary"}>
                        {item.direction}
                      </Badge>
                    </TableCell>
                    <TableCell className="font-mono">
                      {item.counterpartyHex ? (
                        <Link href={`/address/${item.counterpartyHex}`} className="text-sky-700 hover:underline">
                          {shortHex(item.counterpartyHex)}
                        </Link>
                      ) : (
                        "(contract creation)"
                      )}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
          {data.nextCursor ? (
            <div className="text-sm">
              <Link href={`/address/${normalizedHex}?cursor=${encodeURIComponent(data.nextCursor)}`} className="text-sky-700 hover:underline">
                Older
              </Link>
            </div>
          ) : null}
        </div>

        <div className="space-y-2">
          <div className="text-sm font-medium">Failed Transactions (status=0)</div>
          {data.failedHistory.length === 0 ? (
            <div className="rounded-md border bg-slate-50 p-3 text-sm text-muted-foreground">
              No failed transactions in this page.
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Tx Hash</TableHead>
                  <TableHead>Block</TableHead>
                  <TableHead>Index</TableHead>
                  <TableHead>Direction</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.failedHistory.map((item) => (
                  <TableRow key={`failed:${item.txHashHex}`}>
                    <TableCell className="font-mono">
                      <Link href={`/tx/${item.txHashHex}`} className="text-sky-700 hover:underline">
                        {shortHex(item.txHashHex)}
                      </Link>
                    </TableCell>
                    <TableCell>{item.blockNumber.toString()}</TableCell>
                    <TableCell>{item.txIndex}</TableCell>
                    <TableCell>{item.direction}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </div>

        {data.warnings.length > 0 ? (
          <div className="rounded-md border bg-amber-50 p-3 text-sm">
            <div className="mb-1 font-medium">Warnings</div>
            <ul className="list-disc pl-5">
              {data.warnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          </div>
        ) : null}
      </CardContent>
    </Card>
  );
}
