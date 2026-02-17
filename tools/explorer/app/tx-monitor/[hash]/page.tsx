// どこで: Tx監視ページ / 何を: send受理とreceipt結果を分離表示 / なぜ: 送信成功=実行成功の誤解を防ぐため

import { notFound } from "next/navigation";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { getTxMonitorView } from "../../../lib/tx-monitor";
import { isTxHashHex, normalizeHex } from "../../../lib/hex";

export const dynamic = "force-dynamic";

export default async function TxMonitorPage({ params }: { params: Promise<{ hash: string }> }) {
  const { hash } = await params;
  if (!isTxHashHex(hash)) {
    notFound();
  }
  const data = await getTxMonitorView(normalizeHex(hash));

  return (
    <Card>
      <CardHeader>
        <CardTitle>Tx Monitor</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[220px_1fr]">
          <dt className="text-muted-foreground">Input hash</dt>
          <dd className="font-mono break-all">{data.inputHashHex}</dd>
          <dt className="text-muted-foreground">Resolved eth tx hash</dt>
          <dd className="font-mono break-all">{data.resolvedEthTxHashHex ?? "N/A"}</dd>
          <dt className="text-muted-foreground">tx_id</dt>
          <dd className="font-mono break-all">{data.txIdHex ?? "N/A"}</dd>
          <dt className="text-muted-foreground">State</dt>
          <dd>
            <Badge variant={data.state === "included_success" ? "secondary" : "outline"}>{data.state}</Badge>
          </dd>
          <dt className="text-muted-foreground">Summary</dt>
          <dd>{data.summary}</dd>
        </dl>

        <details className="rounded-md border p-3 text-sm">
          <summary className="cursor-pointer font-medium">Raw snapshot</summary>
          <pre className="mt-2 overflow-x-auto text-xs">
            {JSON.stringify(
              {
                tx: data.tx,
                receipt: data.receipt,
                pending: data.pending,
              },
              (_, value) => (typeof value === "bigint" ? value.toString() : value),
              2
            )}
          </pre>
        </details>
      </CardContent>
    </Card>
  );
}
