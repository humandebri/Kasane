// どこで: Receipt詳細ページ / 何を: canister query結果を表示 / なぜ: pruning後でも外部index + queryで確認可能にするため

import Link from "next/link";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { getReceiptView } from "../../../lib/data";
import { toHexLower } from "../../../lib/hex";

export const dynamic = "force-dynamic";

export default async function ReceiptPage({ params }: { params: Promise<{ hash: string }> }) {
  const { hash } = await params;
  const data = await getReceiptView(hash);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Receipt</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {data.tx ? (
          <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[180px_1fr]">
            <dt className="text-muted-foreground">Tx Hash</dt>
            <dd className="font-mono">{data.tx.txHashHex}</dd>
            <dt className="text-muted-foreground">Block Number</dt>
            <dd>
              <Link href={`/blocks/${data.tx.blockNumber.toString()}`} className="text-sky-700 hover:underline">
                {data.tx.blockNumber.toString()}
              </Link>
            </dd>
            <dt className="text-muted-foreground">Tx Index</dt>
            <dd>{data.tx.txIndex}</dd>
          </dl>
        ) : (
          <p className="text-sm">tx is not found in Postgres index.</p>
        )}

        {data.receipt ? (
          <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[180px_1fr]">
            <dt className="text-muted-foreground">Status</dt>
            <dd>
              <Badge variant={data.receipt.status === 1 ? "secondary" : "outline"}>{data.receipt.status}</Badge>
            </dd>
            <dt className="text-muted-foreground">Gas Used</dt>
            <dd>{data.receipt.gas_used.toString()}</dd>
            <dt className="text-muted-foreground">Total Fee</dt>
            <dd>{data.receipt.total_fee.toString()}</dd>
            <dt className="text-muted-foreground">Contract Address</dt>
            <dd className="font-mono">
              {data.receipt.contract_address.length === 0 ? "-" : toHexLower(data.receipt.contract_address[0])}
            </dd>
            <dt className="text-muted-foreground">Logs</dt>
            <dd>{data.receipt.logs.length}</dd>
          </dl>
        ) : (
          <div className="rounded-md border bg-slate-50 p-3">
            <div className="mb-2 text-sm font-medium">Lookup Result</div>
            <pre className="overflow-x-auto text-xs">{JSON.stringify(data.lookupError, null, 2)}</pre>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
