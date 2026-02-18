// どこで: Tx詳細ページ / 何を: ブロック/価値/手数料/ガス価格/ERC-20 transferを表示 / なぜ: Etherscan風の主要確認項目を揃えるため

import Link from "next/link";
import { notFound } from "next/navigation";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { getTxDetailView } from "../../../lib/data";
import { formatGweiFromWei, formatIcpAmountFromWei, formatTokenAmount } from "../../../lib/format";
import { shortHex, toHexLower } from "../../../lib/hex";
import { buildTimelineFromReceiptLogs, type TimelineStep } from "../../../lib/tx_timeline";
import { getTxMonitorView } from "../../../lib/tx-monitor";

export const dynamic = "force-dynamic";

export default async function TxPage({ params }: { params: Promise<{ hash: string }> }) {
  const { hash } = await params;
  const detail = await getTxDetailView(hash);
  if (!detail) {
    notFound();
  }
  const tx = detail.tx;
  const monitor = await getTxMonitorView(tx.txHashHex);
  const timeline = detail.receipt ? buildTimelineFromReceiptLogs(detail.receipt) : null;
  const fromAddressHex = toHexLower(tx.fromAddress);
  const toAddressHex = tx.toAddress ? toHexLower(tx.toAddress) : null;

  return (
    <Card className="border-slate-200 bg-white shadow-sm">
      <CardHeader className="gap-3 border-b border-slate-200">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <CardTitle className="text-xl">Transaction Details</CardTitle>
          <Badge variant={detail.statusLabel === "success" ? "secondary" : detail.statusLabel === "failed" ? "default" : "outline"}>{detail.statusLabel}</Badge>
        </div>
        <p className="font-mono text-xs text-slate-600 break-all">{tx.txHashHex}</p>
      </CardHeader>

      <CardContent className="space-y-4 pt-4">
        <dl className="grid grid-cols-1 gap-y-2 text-sm md:grid-cols-[240px_1fr]">
          <dt className="text-slate-500">Block:</dt>
          <dd>
            <Link href={`/blocks/${tx.blockNumber.toString()}`} className="text-sky-700 hover:underline">
              {tx.blockNumber.toString()}
            </Link>
          </dd>

          <dt className="text-slate-500">ERC-20 Tokens Transferred:</dt>
          <dd>{detail.erc20Transfers.length.toString()}</dd>

          <dt className="text-slate-500">From:</dt>
          <dd className="font-mono break-all">
            <Link href={`/address/${fromAddressHex}`} className="text-sky-700 hover:underline">
              {fromAddressHex}
            </Link>
          </dd>

          <dt className="text-slate-500">To:</dt>
          <dd className="font-mono break-all">
            {toAddressHex ? (
              <Link href={`/address/${toAddressHex}`} className="text-sky-700 hover:underline">
                {toAddressHex}
              </Link>
            ) : (
              "(contract creation)"
            )}
          </dd>

          <dt className="text-slate-500">Caller Principal:</dt>
          <dd className="font-mono break-all">
            {tx.callerPrincipalText ?? "N/A"}
          </dd>

          <dt className="text-slate-500">Value:</dt>
          <dd className="font-mono">{detail.valueWei === null ? "N/A" : formatIcpAmountFromWei(detail.valueWei)}</dd>

          <dt className="text-slate-500">Transaction Fee:</dt>
          <dd className="font-mono">{detail.transactionFeeWei === null ? "N/A" : formatIcpAmountFromWei(detail.transactionFeeWei)}</dd>

          <dt className="text-slate-500">Gas Price (effective_gas_price):</dt>
          <dd className="font-mono">{detail.effectiveGasPriceWei === null ? "N/A" : formatGweiFromWei(detail.effectiveGasPriceWei)}</dd>
          <dt className="text-slate-500">Monitor State:</dt>
          <dd>
            <Badge variant={monitor.state === "included_success" ? "secondary" : monitor.state === "included_failed" ? "default" : "outline"}>
              {monitor.state}
            </Badge>
          </dd>
          <dt className="text-slate-500">Monitor Summary:</dt>
          <dd>{monitor.summary}</dd>
        </dl>

        {detail.erc20Transfers.length > 0 ? (
          <div className="space-y-2 rounded-md border border-slate-200 p-3">
            <div className="text-sm font-medium">ERC-20 Transfers</div>
            <div className="space-y-2">
              {detail.erc20Transfers.map((item) => (
                <div key={`${item.logIndex}:${item.tokenAddressHex}:${item.fromAddressHex}:${item.toAddressHex}`} className="rounded-md border border-slate-200 p-2 text-xs">
                  <div className="font-mono break-all">token: <Link href={`/address/${item.tokenAddressHex}`} className="text-sky-700 hover:underline">{item.tokenAddressHex}</Link></div>
                  <div className="font-mono break-all">from: <Link href={`/address/${item.fromAddressHex}`} className="text-sky-700 hover:underline">{item.fromAddressHex}</Link></div>
                  <div className="font-mono break-all">to: <Link href={`/address/${item.toAddressHex}`} className="text-sky-700 hover:underline">{item.toAddressHex}</Link></div>
                  <div className="font-mono break-all">
                    amount: {formatTokenAmount(item.amount, item.tokenDecimals)} {item.tokenSymbol ?? ""}
                    {item.tokenDecimals === null ? " (raw)" : ""}
                  </div>
                </div>
              ))}
            </div>
          </div>
        ) : null}

        <div className="space-y-3 rounded-lg border border-slate-200 p-3">
          <div className="text-sm font-medium">Receipt Details</div>
          {detail.receipt ? (
            <>
              <dl className="grid grid-cols-1 gap-y-2 text-sm md:grid-cols-[190px_1fr]">
                <dt className="text-slate-500">Status</dt>
                <dd>
                  <Badge variant={detail.receipt.status === 1 ? "secondary" : "default"}>{detail.receipt.status}</Badge>
                </dd>
                <dt className="text-slate-500">Gas Used</dt>
                <dd>{detail.receipt.gas_used.toString()}</dd>
                <dt className="text-slate-500">Total Fee</dt>
                <dd>{detail.receipt.total_fee.toString()}</dd>
                <dt className="text-slate-500">Contract Address</dt>
                <dd className="font-mono text-xs break-all">
                  {detail.receipt.contract_address.length === 0 ? "-" : toHexLower(detail.receipt.contract_address[0])}
                </dd>
                <dt className="text-slate-500">Logs</dt>
                <dd>{detail.receipt.logs.length}</dd>
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
              <pre className="overflow-x-auto text-xs">{JSON.stringify(detail.receiptLookupError, null, 2)}</pre>
            </div>
          )}
        </div>

        <div className="font-mono text-xs text-slate-500">tx: {shortHex(tx.txHashHex)} / index: {tx.txIndex}</div>
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
