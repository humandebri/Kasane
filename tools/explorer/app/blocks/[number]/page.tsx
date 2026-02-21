// どこで: ブロック詳細ページ / 何を: ブロック概要と収容tx一覧をEtherscan風に表示 / なぜ: 調査時にブロック状態を即把握できるようにするため

import Link from "next/link";
import { notFound } from "next/navigation";
import { Clock3 } from "lucide-react";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { getBlockView } from "../../../lib/data";
import { calcRoundedBps, formatEthFromWei, formatGweiFromWei, formatTimestampWithRelativeUtc } from "../../../lib/format";

export const dynamic = "force-dynamic";
const FEE_RECIPIENT = "0x6b9b5fd62cc66fc9fef74210c9298b1b6bcbfc52";

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
  const timestampView = formatTimestampWithRelativeUtc(data.db?.block.timestamp ?? null);

  if (!data.db) {
    notFound();
  }

  return (
    <Card className=" bg-white shadow-sm">
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
          <dt className="text-slate-500">Timestamp:</dt>
          <dd>
            {timestampView ? (
              <span className="inline-flex items-center gap-1.5">
                <Clock3 className="h-3.5 w-3.5 text-slate-500" />
                <span>{`${timestampView.relative} (${timestampView.absolute})`}</span>
              </span>
            ) : (
              "N/A"
            )}
          </dd>
          <dt className="text-slate-500">Transactions:</dt>
          <dd>
            <Link href={`/txs?block=${data.db.block.number.toString()}`} className="text-sky-700 hover:underline">
              {`${data.db.block.txCount} transactions`}
            </Link>
          </dd>
          <dt className="text-slate-500">Fee Recipient:</dt>
          <dd>
            <Link href={`/address/${FEE_RECIPIENT}`} className="font-mono text-sky-700 hover:underline">
              {FEE_RECIPIENT}
            </Link>
          </dd>
          <dt className="text-slate-500">Block Reward:</dt>
          <dd className="font-mono">
            {formatBlockReward(data.rpcGas?.totalFeesWei, data.rpcGas?.burntFeesWei, data.rpcGas?.priorityFeesWei)}
          </dd>
          <dt className="text-slate-500">Size:</dt>
          <dd>-</dd>
          <dt className="text-slate-500">Gas Used</dt>
          <dd>{formatGasUsed(data.rpcGas?.gasUsed, data.rpcGas?.gasLimit)}</dd>
          <dt className="text-slate-500">Gas Limit</dt>
          <dd>{formatInteger(data.rpcGas?.gasLimit)}</dd>
          <dt className="text-slate-500">Base Fee Per Gas</dt>
          <dd>{formatBaseFee(data.rpcGas?.baseFeePerGasWei)}</dd>
          <dt className="text-slate-500">Base Fee</dt>
          <dd>{formatBaseFeePortion(data.rpcGas?.burntFeesWei)}</dd>
        </dl>

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

function formatBaseFeePortion(burntFeesWei: bigint | null | undefined): string {
  if (burntFeesWei === null || burntFeesWei === undefined) {
    return "-";
  }
  return formatEthFromWei(burntFeesWei);
}

function formatBlockReward(
  totalFeesWei: bigint | null | undefined,
  baseFeePortionWei: bigint | null | undefined,
  priorityFeesWei: bigint | null | undefined
): string {
  if (totalFeesWei === null || totalFeesWei === undefined || baseFeePortionWei === null || baseFeePortionWei === undefined) {
    return "-";
  }
  const totalText = formatEthFromWei(totalFeesWei);
  const baseText = formatEthFromWei(baseFeePortionWei);
  const priorityText = priorityFeesWei === null || priorityFeesWei === undefined ? "N/A" : formatEthFromWei(priorityFeesWei);
  return `${totalText} (0 + ${baseText} + ${priorityText})`;
}
