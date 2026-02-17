// どこで: ブロック詳細ページ / 何を: ブロック概要と収容tx一覧をEtherscan風に表示 / なぜ: 調査時にブロック状態を即把握できるようにするため

import Link from "next/link";
import { notFound } from "next/navigation";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../../components/ui/table";
import { getBlockView } from "../../../lib/data";
import { calcRoundedBps, formatEthFromWei, formatGweiFromWei, formatTimestampUtc, receiptStatusLabel } from "../../../lib/format";
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
    <Card className="border-slate-200 bg-white shadow-sm">
      <CardHeader className="gap-3 border-b border-slate-200">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <CardTitle className="text-xl">Block {data.db.block.number.toString()}</CardTitle>
          <Badge variant={data.rpcExists ? "secondary" : "outline"}>{data.rpcExists ? "RPC visible" : "RPC missing"}</Badge>
        </div>
      </CardHeader>

      <CardContent className="space-y-4 pt-4">
        <dl className="grid grid-cols-1 gap-y-2 text-sm md:grid-cols-[190px_1fr]">
          <dt className="text-slate-500">Hash</dt>
          <dd className="font-mono text-xs break-all">{data.db.block.hashHex ?? "-"}</dd>
          <dt className="text-slate-500">Timestamp (UTC)</dt>
          <dd>{formatTimestampUtc(data.db.block.timestamp)}</dd>
          <dt className="text-slate-500">Tx Count (DB)</dt>
          <dd>{data.db.block.txCount}</dd>
          <dt className="text-slate-500">Gas Used</dt>
          <dd>{formatGasUsed(data.rpcGas?.gasUsed, data.rpcGas?.gasLimit)}</dd>
          <dt className="text-slate-500">Gas Limit</dt>
          <dd>{formatInteger(data.rpcGas?.gasLimit)}</dd>
          <dt className="text-slate-500">Base Fee Per Gas</dt>
          <dd>{formatBaseFee(data.rpcGas?.baseFeePerGasWei)}</dd>
          <dt className="text-slate-500">Burnt Fees</dt>
          <dd>{formatBurntFees(data.rpcGas?.burntFeesWei)}</dd>
          <dt className="text-slate-500">Gas vs Target</dt>
          <dd>{formatGasTargetDelta(data.rpcGas?.gasUsed, data.rpcGas?.gasLimit)}</dd>
        </dl>

        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Tx Hash</TableHead>
              <TableHead>Index</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Caller Principal</TableHead>
              <TableHead>Details</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {data.db.txs.map((tx) => {
              const status = receiptStatusLabel(tx.receiptStatus);
              return (
                <TableRow key={tx.txHashHex}>
                  <TableCell className="font-mono text-xs">
                    <Link href={`/tx/${tx.txHashHex}`} className="text-sky-700 hover:underline" title={tx.txHashHex}>
                      {shortHex(tx.txHashHex)}
                    </Link>
                  </TableCell>
                  <TableCell>{tx.txIndex}</TableCell>
                  <TableCell>
                    <Badge variant={status === "success" ? "secondary" : status === "failed" ? "default" : "outline"}>{status}</Badge>
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    {tx.callerPrincipalText ? (
                      <Link href={`/principal/${encodeURIComponent(tx.callerPrincipalText)}`} className="text-sky-700 hover:underline">
                        {tx.callerPrincipalText}
                      </Link>
                    ) : (
                      "-"
                    )}
                  </TableCell>
                  <TableCell>
                    <Link href={`/tx/${tx.txHashHex}`} className="text-sky-700 hover:underline">
                      open
                    </Link>
                  </TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

function formatInteger(value: bigint | null | undefined): string {
  if (value === null || value === undefined) {
    return "-";
  }
  return value.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

function formatGasUsed(gasUsed: bigint | null | undefined, gasLimit: bigint | null | undefined): string {
  if (gasUsed === null || gasUsed === undefined) {
    return "-";
  }
  if (gasLimit === null || gasLimit === undefined || gasLimit === 0n) {
    return formatInteger(gasUsed);
  }
  const bps = calcRoundedBps(gasUsed, gasLimit);
  if (bps === null) {
    return formatInteger(gasUsed);
  }
  const whole = bps / 100n;
  const fraction = (bps % 100n).toString().padStart(2, "0");
  return `${formatInteger(gasUsed)} (${whole.toString()}.${fraction}%)`;
}

function formatBaseFee(baseFeePerGasWei: bigint | null | undefined): string {
  if (baseFeePerGasWei === null || baseFeePerGasWei === undefined) {
    return "-";
  }
  return `${formatEthFromWei(baseFeePerGasWei)} (${formatGweiFromWei(baseFeePerGasWei)})`;
}

function formatBurntFees(burntFeesWei: bigint | null | undefined): string {
  if (burntFeesWei === null || burntFeesWei === undefined) {
    return "-";
  }
  return formatEthFromWei(burntFeesWei);
}

function formatGasTargetDelta(gasUsed: bigint | null | undefined, gasLimit: bigint | null | undefined): string {
  if (gasUsed === null || gasUsed === undefined || gasLimit === null || gasLimit === undefined) {
    return "-";
  }
  const gasTarget = gasLimit / 2n;
  if (gasTarget === 0n) {
    return "-";
  }
  const deltaBps = calcRoundedBps(gasUsed - gasTarget, gasTarget);
  if (deltaBps === null) {
    return "-";
  }
  const sign = deltaBps > 0n ? "+" : "";
  const abs = deltaBps < 0n ? -deltaBps : deltaBps;
  const whole = abs / 100n;
  const fraction = (abs % 100n).toString().padStart(2, "0");
  return `${sign}${whole.toString()}.${fraction}% vs Gas Target`;
}
