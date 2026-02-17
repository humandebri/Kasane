// どこで: Tx詳細ページ / 何を: ブロック/価値/手数料/ガス価格/ERC-20 transferを表示 / なぜ: Etherscan風の主要確認項目を揃えるため

import Link from "next/link";
import { notFound } from "next/navigation";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { Button } from "../../../components/ui/button";
import { getTxDetailView } from "../../../lib/data";
import { formatIcpAmountFromWei, formatTokenAmount } from "../../../lib/format";
import { shortHex } from "../../../lib/hex";
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

          <dt className="text-slate-500">Value:</dt>
          <dd className="font-mono">{detail.valueWei === null ? "N/A" : formatIcpAmountFromWei(detail.valueWei)}</dd>

          <dt className="text-slate-500">Transaction Fee:</dt>
          <dd className="font-mono">{detail.transactionFeeWei === null ? "N/A" : formatIcpAmountFromWei(detail.transactionFeeWei)}</dd>

          <dt className="text-slate-500">Gas Price:</dt>
          <dd className="font-mono">{detail.gasPriceWei === null ? "N/A" : `${formatIcpAmountFromWei(detail.gasPriceWei)} / gas`}</dd>
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

        <div className="flex flex-wrap gap-2">
          <Link href={`/receipt/${tx.txHashHex}`} className="inline-flex">
            <Button type="button" variant="secondary" className="rounded-full">View Receipt</Button>
          </Link>
        </div>

        <div className="font-mono text-xs text-slate-500">tx: {shortHex(tx.txHashHex)} / index: {tx.txIndex}</div>
      </CardContent>
    </Card>
  );
}
