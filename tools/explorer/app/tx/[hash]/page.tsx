// どこで: Tx詳細ページ / 何を: indexer保存済みtx位置情報を表示 / なぜ: tx単位の追跡導線を提供するため

import Link from "next/link";
import { notFound } from "next/navigation";
import { Principal } from "@dfinity/principal";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { getTxView } from "../../../lib/data";

export const dynamic = "force-dynamic";

export default async function TxPage({ params }: { params: Promise<{ hash: string }> }) {
  const { hash } = await params;
  const tx = await getTxView(hash);
  if (!tx) {
    notFound();
  }
  const callerPrincipal = toCallerPrincipalText(tx.callerPrincipal);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Transaction</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[180px_1fr]">
          <dt className="text-muted-foreground">Tx Hash</dt>
          <dd className="font-mono">{tx.txHashHex}</dd>
          <dt className="text-muted-foreground">Block Number</dt>
          <dd>
            <Link href={`/blocks/${tx.blockNumber.toString()}`} className="text-sky-700 hover:underline">
              {tx.blockNumber.toString()}
            </Link>
          </dd>
          <dt className="text-muted-foreground">Tx Index</dt>
          <dd>{tx.txIndex}</dd>
          <dt className="text-muted-foreground">Caller Principal</dt>
          <dd className="font-mono">
            {callerPrincipal === "-" ? (
              "-"
            ) : (
              <Link href={`/principal/${encodeURIComponent(callerPrincipal)}`} className="text-sky-700 hover:underline">
                {callerPrincipal}
              </Link>
            )}
          </dd>
        </dl>
        <div>
          <Link href={`/receipt/${tx.txHashHex}`} className="text-sky-700 hover:underline">
            View Receipt
          </Link>
        </div>
        <div>
          <Link href={`/tx-monitor/${tx.txHashHex}`} className="text-sky-700 hover:underline">
            Open Tx Monitor
          </Link>
        </div>
      </CardContent>
    </Card>
  );
}

function toCallerPrincipalText(callerPrincipal: Buffer | null): string {
  if (!callerPrincipal) {
    return "-";
  }
  return Principal.fromUint8Array(callerPrincipal).toText();
}
