// どこで: Tx詳細ページ / 何を: ブロック/価値/手数料/ガス価格/ERC-20 transferを表示 / なぜ: Etherscan風の主要確認項目を揃えるため

import Link from "next/link";
import { notFound } from "next/navigation";
import type { ReactNode } from "react";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent, CardHeader } from "../../../components/ui/card";
import { Erc20TransfersPanel } from "../../../components/erc20-transfers-panel";
import { getTxDetailView } from "../../../lib/data";
import { formatGweiFromWei, formatIcpAmountFromWei, formatTimestampUtc, receiptStatusLabel } from "../../../lib/format";
import { shortHex, toHexLower } from "../../../lib/hex";
import { buildTimelineFromReceiptLogs, type TimelineStep } from "../../../lib/tx_timeline";

export const dynamic = "force-dynamic";

type ReceiptTab = "overview" | "logs";

export default async function TxPage({
  params,
  searchParams,
}: {
  params: Promise<{ hash: string }>;
  searchParams: Promise<{ tab?: string }>;
}) {
  const { hash } = await params;
  const { tab } = await searchParams;
  const detail = await getTxDetailView(hash);
  if (!detail) {
    notFound();
  }
  const tx = detail.tx;
  const receiptTab: ReceiptTab = tab === "logs" ? "logs" : "overview";
  const statusLabel = receiptStatusLabel(tx.receiptStatus);
  const timeline = detail.receipt ? buildTimelineFromReceiptLogs(detail.receipt) : null;
  const fromAddressHex = toHexLower(tx.fromAddress);
  const toAddressHex = tx.toAddress ? toHexLower(tx.toAddress) : null;

  return (
    <Card className="border-slate-200 bg-white shadow-sm">
      <CardHeader className="gap-3 border-b border-slate-200">
        <div className="flex flex-wrap items-center justify-end gap-3">
          <Badge variant={statusLabel === "success" ? "secondary" : statusLabel === "failed" ? "default" : "outline"}>{statusLabel}</Badge>
        </div>
        <p className="font-mono text-xs text-slate-600 break-all">{tx.txHashHex}</p>
      </CardHeader>

      <CardContent className="space-y-4 pt-4">
        <dl className="grid grid-cols-1 gap-y-2 text-sm md:grid-cols-[240px_1fr]">
          <dt className="text-slate-500">Transaction Hash:</dt>
          <dd className="font-mono break-all">{tx.txHashHex}</dd>

          <dt className="text-slate-500">Status:</dt>
          <dd>
            <Badge variant={statusLabel === "success" ? "secondary" : statusLabel === "failed" ? "default" : "outline"}>
              {statusLabel}
            </Badge>
          </dd>

          <dt className="text-slate-500">Block:</dt>
          <dd>
            <Link href={`/blocks/${tx.blockNumber.toString()}`} className="text-sky-700 hover:underline">
              {tx.blockNumber.toString()}
            </Link>
          </dd>

          <dt className="text-slate-500">Timestamp:</dt>
          <dd>{tx.blockTimestamp === null ? "N/A" : formatTimestampUtc(tx.blockTimestamp)}</dd>

          <dt className="text-slate-500">From:</dt>
          <dd className="font-mono break-all">
            <Link href={`/address/${fromAddressHex}`} className="text-sky-700 hover:underline">
              {fromAddressHex}
            </Link>
          </dd>

          <dt className="text-slate-500">Interacted With (To):</dt>
          <dd className="font-mono break-all">
            {toAddressHex ? (
              <Link href={`/address/${toAddressHex}`} className="text-sky-700 hover:underline">
                {toAddressHex}
              </Link>
            ) : (
              "(contract creation)"
            )}
          </dd>

          <dt className="text-slate-500">ERC-20 Tokens Transferred:</dt>
          <dd>
            {detail.erc20Transfers.length > 0 ? (
              <Link href="#erc20-transfers" className="text-sky-700 hover:underline">
                {detail.erc20Transfers.length.toString()}
              </Link>
            ) : (
              "0"
            )}
          </dd>

          <dt className="text-slate-500">Value:</dt>
          <dd className="font-mono">{detail.valueWei === null ? "N/A" : formatIcpAmountFromWei(detail.valueWei)}</dd>

          <dt className="text-slate-500">Transaction Fee:</dt>
          <dd className="font-mono">{detail.transactionFeeWei === null ? "N/A" : formatIcpAmountFromWei(detail.transactionFeeWei)}</dd>

          <dt className="text-slate-500">Gas Price:</dt>
          <dd className="font-mono">{detail.effectiveGasPriceWei === null ? "N/A" : formatGweiFromWei(detail.effectiveGasPriceWei)}</dd>

          <dt className="text-slate-500">Caller Principal:</dt>
          <dd className="font-mono break-all">{tx.callerPrincipalText ?? "N/A"}</dd>
        </dl>

        {detail.erc20Transfers.length > 0 ? (
          <div id="erc20-transfers">
            <Erc20TransfersPanel transfers={detail.erc20Transfers} />
          </div>
        ) : null}

        <div className="space-y-3 rounded-lg border border-slate-200 p-3">
          <div className="flex items-center justify-between gap-2">
            <div className="text-sm font-medium">Receipt Details</div>
            {detail.receipt ? (
              <div className="flex flex-wrap gap-2 text-xs">
                <Link
                  href={buildReceiptTabHref(tx.txHashHex, "overview")}
                  scroll={false}
                  className={`rounded-md border px-3 py-1 ${receiptTab === "overview" ? "border-sky-300 bg-sky-50 text-sky-700" : "border-slate-300 bg-white text-slate-700"}`}
                >
                  Overview
                </Link>
                <Link
                  href={buildReceiptTabHref(tx.txHashHex, "logs")}
                  scroll={false}
                  className={`rounded-md border px-3 py-1 ${receiptTab === "logs" ? "border-sky-300 bg-sky-50 text-sky-700" : "border-slate-300 bg-white text-slate-700"}`}
                >
                  Logs ({detail.receipt.logs.length})
                </Link>
              </div>
            ) : null}
          </div>
          {detail.receipt ? (
            <>
              {receiptTab === "overview" ? (
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
              ) : null}
              {timeline && receiptTab === "logs" ? (
                <div className="space-y-3 rounded-lg border border-slate-200 p-3">
                  <div className="text-sm font-medium">Transaction Receipt Event Logs</div>
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
    <div className="rounded-md border border-slate-200 p-3">
      <div className="flex items-start gap-3">
        <div className="min-w-10 text-right">
          <div className="text-2xl leading-none font-semibold text-slate-700">{step.index}</div>
          <div className="mt-1 text-[10px] uppercase tracking-wide text-slate-400">log</div>
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <Badge variant={step.type === "unknown" ? "outline" : "secondary"}>{step.type}</Badge>
            <span className="rounded bg-slate-100 px-1.5 py-0.5 font-mono">{step.protocol}</span>
          </div>
          <div className="mt-2 grid grid-cols-1 gap-1 text-xs md:grid-cols-[90px_1fr]">
            <div className="text-slate-500">Address</div>
            <div className="font-mono break-all">
              <Link href={`/address/${step.addressHex}`} className="text-sky-700 hover:underline">
                {step.addressHex}
              </Link>
            </div>
            <div className="text-slate-500">Topic0</div>
            <div className="font-mono break-all">
              {step.topic0Hex ? <span className="text-slate-700">{step.topic0Hex}</span> : <span className="text-muted-foreground">-</span>}
            </div>
          </div>
          <div className="mt-2 rounded border border-slate-100 bg-slate-50 p-2 text-sm break-all">
            {linkifyAddresses(step.summary)}
          </div>
          {step.type === "unknown" ? (
            <details className="mt-2 rounded-md border bg-slate-50 p-2 text-xs">
              <summary className="cursor-pointer">raw</summary>
              <pre className="mt-1 overflow-x-auto">{JSON.stringify(step.raw, null, 2)}</pre>
            </details>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function linkifyAddresses(value: string): ReactNode[] {
  const out: ReactNode[] = [];
  const pattern = /0x[a-fA-F0-9]{40}/g;
  let cursor = 0;
  let match: RegExpExecArray | null = pattern.exec(value);
  while (match) {
    const full = match[0];
    const start = match.index;
    if (start > cursor) {
      out.push(value.slice(cursor, start));
    }
    out.push(
      <Link key={`${full}:${start}`} href={`/address/${full.toLowerCase()}`} className="font-mono text-sky-700 hover:underline">
        {full}
      </Link>
    );
    cursor = start + full.length;
    match = pattern.exec(value);
  }
  if (cursor < value.length) {
    out.push(value.slice(cursor));
  }
  return out;
}

function buildReceiptTabHref(txHashHex: string, tab: ReceiptTab): string {
  const query = new URLSearchParams();
  query.set("tab", tab);
  return `/tx/${txHashHex}?${query.toString()}`;
}
