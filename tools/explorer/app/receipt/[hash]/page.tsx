// どこで: Receipt詳細ページ / 何を: 実行結果とログタイムラインをEtherscan風に整理表示 / なぜ: 送信成功と実行成功の差分を見落とさないため

import Link from "next/link";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { getReceiptView } from "../../../lib/data";
import { shortHex, toHexLower } from "../../../lib/hex";
import { buildTimelineFromReceiptLogs, type TimelineStep } from "../../../lib/tx_timeline";

export const dynamic = "force-dynamic";

export default async function ReceiptPage({ params }: { params: Promise<{ hash: string }> }) {
  const { hash } = await params;
  const data = await getReceiptView(hash);
  const timeline = data.receipt ? buildTimelineFromReceiptLogs(data.receipt) : null;

  return (
    <Card className="border-slate-200 bg-white shadow-sm">
      <CardHeader className="gap-3 border-b border-slate-200">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <CardTitle className="text-xl">Receipt Details</CardTitle>
          {data.receipt ? (
            <Badge variant={data.receipt.status === 1 ? "secondary" : "default"}>{data.receipt.status === 1 ? "success" : "failed"}</Badge>
          ) : null}
        </div>
        {data.tx ? <p className="font-mono text-xs text-slate-600 break-all">{data.tx.txHashHex}</p> : null}
      </CardHeader>

      <CardContent className="space-y-4 pt-4">
        {data.tx ? (
          <dl className="grid grid-cols-1 gap-y-2 text-sm md:grid-cols-[190px_1fr]">
            <dt className="text-slate-500">Tx Hash</dt>
            <dd className="font-mono text-xs break-all">{data.tx.txHashHex}</dd>
            <dt className="text-slate-500">Block Number</dt>
            <dd>
              <Link href={`/blocks/${data.tx.blockNumber.toString()}`} className="text-sky-700 hover:underline">
                {data.tx.blockNumber.toString()}
              </Link>
            </dd>
            <dt className="text-slate-500">Tx Index</dt>
            <dd>{data.tx.txIndex}</dd>
          </dl>
        ) : (
          <p className="text-sm">tx is not found in Postgres index.</p>
        )}

        {data.receipt ? (
          <>
            <dl className="grid grid-cols-1 gap-y-2 text-sm md:grid-cols-[190px_1fr]">
              <dt className="text-slate-500">Status</dt>
              <dd>
                <Badge variant={data.receipt.status === 1 ? "secondary" : "default"}>{data.receipt.status}</Badge>
              </dd>
              <dt className="text-slate-500">Gas Used</dt>
              <dd>{data.receipt.gas_used.toString()}</dd>
              <dt className="text-slate-500">Total Fee</dt>
              <dd>{data.receipt.total_fee.toString()}</dd>
              <dt className="text-slate-500">Contract Address</dt>
              <dd className="font-mono text-xs break-all">
                {data.receipt.contract_address.length === 0 ? "-" : toHexLower(data.receipt.contract_address[0])}
              </dd>
              <dt className="text-slate-500">Logs</dt>
              <dd>{data.receipt.logs.length}</dd>
            </dl>

            {timeline ? (
              <div className="space-y-3 rounded-lg border border-slate-200 p-3">
                <div className="text-sm font-medium">Timeline</div>
                <div className="grid grid-cols-1 gap-2 text-xs md:grid-cols-4">
                  <div>borrow: {timeline.counters.borrow}</div>
                  <div>swap: {timeline.counters.swap}</div>
                  <div>repay: {timeline.counters.repay}</div>
                  <div>unknown: {timeline.counters.unknown}</div>
                </div>
                {timeline.steps.length === 0 ? (
                  <p className="text-sm text-muted-foreground">No timeline events.</p>
                ) : (
                  <div className="space-y-2">
                    {timeline.steps.map((step) => (
                      <TimelineRow key={`${step.index}:${step.addressHex}:${step.topic0Hex ?? "none"}`} step={step} />
                    ))}
                  </div>
                )}
                <ul className="list-disc pl-5 text-xs text-muted-foreground">
                  {timeline.notes.map((note) => (
                    <li key={note}>{note}</li>
                  ))}
                </ul>
              </div>
            ) : null}
          </>
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

function TimelineRow({ step }: { step: TimelineStep }) {
  return (
    <div className="rounded-md border border-slate-200 p-2">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className="text-muted-foreground">#{step.index}</span>
        <Badge variant={step.type === "unknown" ? "outline" : "secondary"}>{step.type}</Badge>
        <span className="font-mono">{step.protocol}</span>
        <span className="font-mono" title={step.addressHex}>
          {shortHex(step.addressHex)}
        </span>
      </div>
      <div className="mt-1 text-sm">{step.summary}</div>
      {step.type === "unknown" ? (
        <details className="mt-2 rounded-md border bg-slate-50 p-2 text-xs">
          <summary className="cursor-pointer">raw</summary>
          <pre className="mt-1 overflow-x-auto">{JSON.stringify(step.raw, null, 2)}</pre>
        </details>
      ) : null}
    </div>
  );
}
