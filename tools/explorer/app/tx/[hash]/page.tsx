// どこで: Tx詳細ページ / 何を: ブロック/価値/手数料/ガス価格/ERC-20 transferを表示 / なぜ: Etherscan風の主要確認項目を揃えるため

import Link from "next/link";
import { notFound } from "next/navigation";
import { AlertTriangle, Clock3 } from "lucide-react";
import type { ReactNode } from "react";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent } from "../../../components/ui/card";
import { Erc20TransfersPanel, type Erc20TransferRowView } from "../../../components/erc20-transfers-panel";
import { StatusSuccessIcon } from "../../../components/status-success-icon";
import { TxLogDataToggle } from "../../../components/tx-log-data-toggle";
import { getTxDetailView } from "../../../lib/data";
import {
  calcRoundedBps,
  formatGweiFromWei,
  formatIcpAmountFromWei,
  formatTimestampWithRelativeUtc,
  formatTokenAmount,
  receiptStatusLabel,
} from "../../../lib/format";
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
  const timestampView = formatTimestampWithRelativeUtc(tx.blockTimestamp);
  const fromAddressHex = toHexLower(tx.fromAddress);
  const toAddressHex = tx.toAddress ? toHexLower(tx.toAddress) : null;
  const erc20TransfersView: Erc20TransferRowView[] = detail.erc20Transfers.map((row) => ({
    logIndex: row.logIndex,
    tokenAddressHex: row.tokenAddressHex,
    tokenSymbol: row.tokenSymbol,
    fromAddressHex: row.fromAddressHex,
    toAddressHex: row.toAddressHex,
    amountText: formatTokenAmount(row.amount, row.tokenDecimals),
    isRawAmount: row.tokenDecimals === null,
  }));
  const dividedRowLabelClass = "border-b border-slate-200 pb-4 text-slate-500";
  const dividedRowValueClass = "border-b border-slate-200 pb-4 font-mono break-all";

  return (
    <>
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

      {receiptTab === "overview" || !detail.receipt ? (
      <Card className="border-slate-200 bg-white shadow-sm py-0">
        <CardContent className="space-y-4 py-4">
        <dl className="grid grid-cols-1 gap-y-4 text-sm md:grid-cols-[240px_1fr]">
          <dt className="text-slate-500">Transaction Hash:</dt>
          <dd className="font-mono break-all">{tx.txHashHex}</dd>

          <dt className="text-slate-500">Status:</dt>
          <dd>
            {statusLabel === "success" ? (
              <Badge variant="outline" className="border-emerald-200 bg-emerald-50 text-emerald-700">
                <StatusSuccessIcon className="mr-1 h-3 w-3" />
                success
              </Badge>
            ) : statusLabel === "failed" ? (
              <Badge variant="outline" className="border-rose-200 bg-rose-50 text-rose-700">
                <AlertTriangle className="mr-1 h-3 w-3" />
                failed
              </Badge>
            ) : (
              <Badge variant="outline">{statusLabel}</Badge>
            )}
          </dd>

          <dt className="text-slate-500">Block:</dt>
          <dd>
            <Link href={`/blocks/${tx.blockNumber.toString()}`} className="text-sky-700 hover:underline">
              {tx.blockNumber.toString()}
            </Link>
          </dd>

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

          <dt className="text-slate-500">From:</dt>
          <dd className="font-mono break-all">
            <Link href={`/address/${fromAddressHex}`} className="text-sky-700 hover:underline">
              {fromAddressHex}
            </Link>
          </dd>

          <dt className={dividedRowLabelClass}>Interacted With (To):</dt>
          <dd className={dividedRowValueClass}>
            {toAddressHex ? (
              <Link href={`/address/${toAddressHex}`} className="text-sky-700 hover:underline">
                {toAddressHex}
              </Link>
            ) : tx.createdContractAddress ? (
              <Link href={`/address/${toHexLower(tx.createdContractAddress)}`} className="text-sky-700 hover:underline">
                Contract Creation
              </Link>
            ) : (
              "Contract Creation"
            )}
          </dd>

          <dt className="text-slate-500">ERC-20 Tokens Transferred:</dt>
          <dd>
            {erc20TransfersView.length > 0 ? (
              <Erc20TransfersPanel transfers={erc20TransfersView} />
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
        </CardContent>
      </Card>
      ) : null}
      {receiptTab === "overview" || !detail.receipt ? (
        <Card className="border-slate-200 bg-white shadow-sm py-4">
          <CardContent className="space-y-3">
            {detail.receipt ? (
              <dl className="grid grid-cols-1 gap-y-2 text-sm md:grid-cols-[240px_1fr]">
                <dt className="text-slate-500">Gas Limit &amp; Usage by Txn:</dt>
                <dd className="font-mono">{formatGasLimitAndUsage(detail.gasLimit, detail.gasUsed)}</dd>
                <dt className="text-slate-500">Gas Fees:</dt>
                <dd className="font-mono">
                  {`Base: ${formatOptionalGwei(detail.baseFeePerGasWei)} | Max: ${formatOptionalGwei(detail.maxFeePerGasWei)} | Max Priority: ${formatOptionalGwei(detail.maxPriorityFeePerGasWei)}`}
                </dd>
              </dl>
            ) : (
              <div className="rounded-md border bg-slate-50 p-3">
                <div className="mb-2 text-sm font-medium">Lookup Result</div>
                <pre className="overflow-x-auto text-xs">{stringifyWithBigInt(detail.receiptLookupError)}</pre>
              </div>
            )}
          </CardContent>
        </Card>
      ) : null}
      {receiptTab === "logs" && timeline ? (
        <Card className="border-slate-200 bg-white shadow-sm py-2">
          <CardContent className="space-y-3 pt-4">
            <div className="text-sm font-medium">{`Transaction Receipt Event Logs (${detail.receipt?.logs.length ?? 0})`}</div>
  
            {timeline.steps.length === 0 ? (
              <p className="text-sm text-muted-foreground">No timeline events.</p>
            ) : (
              <div className="space-y-4">
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
          </CardContent>
        </Card>
      ) : null}
    </>
  );
}

function buildReceiptTabHref(txHashHex: string, tab: ReceiptTab): string {
  const query = new URLSearchParams();
  query.set("tab", tab);
  return `/tx/${txHashHex}?${query.toString()}`;
}

function TimelineRow({ step }: { step: TimelineStep }) {
  return (
    <div className="border-b border-slate-200 py-3">
      <div className="space-y-6">
        <div className="grid grid-cols-1 gap-y-3 text-xs md:grid-cols-[90px_1fr]">
          <div className="text-slate-500">Log Index</div>
          <div className="font-mono text-slate-700">{step.index}</div>
        </div>

        <div className="grid grid-cols-1 gap-y-3 text-xs md:grid-cols-[90px_1fr]">
          <div className="text-slate-500">Address</div>
          <div className="font-mono break-all">
            <Link href={`/address/${step.addressHex}`} className="text-sky-700 hover:underline">
              {step.addressHex}
            </Link>
          </div>
        </div>

        <div className="grid grid-cols-1 gap-y-3 text-xs md:grid-cols-[90px_1fr]">
          <div className="text-slate-500">Name</div>
          <div className="font-mono text-slate-700">{formatEventName(step)}</div>
        </div>

        <div className="grid grid-cols-1 gap-y-3 text-xs md:grid-cols-[90px_1fr]">
          <div className="text-slate-500">Topics</div>
          <div className="space-y-3">
            {step.raw.topicsHex.length === 0 ? (
              <div className="font-mono text-slate-500">-</div>
            ) : (
              step.raw.topicsHex.map((topic, index) => (
                <div key={`${step.index}:topic:${index.toString()}`} className="font-mono break-all text-slate-700">
                  <span className="mr-2 inline-flex min-w-5 justify-center rounded bg-slate-100 px-1 text-[10px] text-slate-500">
                    {formatTopicLabel(step.topic0Hex, index)}
                  </span>
                  {renderTopicValue(step.topic0Hex, index, topic)}
                </div>
              ))
            )}
          </div>
        </div>

        <div className="grid grid-cols-1 gap-y-3 text-xs md:grid-cols-[90px_1fr]">
          <div className="text-slate-500 pt-3">Data</div>
          <div className="rounded   bg-slate-100 p-2 font-mono break-all ">
            <TxLogDataToggle dataHex={step.raw.dataHex} />
          </div>
        </div>

        {step.type === "unknown" ? (
          <div className="grid grid-cols-1 gap-y-3 text-xs md:grid-cols-[90px_1fr]">
            <div className="text-slate-500">Raw</div>
            <details className="rounded bg-slate-100 p-2">
              <summary className="cursor-pointer select-none text-[11px] font-medium text-slate-600">Show raw log</summary>
              <pre className="mt-2 overflow-x-auto font-mono text-[11px] leading-5 text-slate-700">
                {JSON.stringify(
                  {
                    logIndex: step.index,
                    address: step.addressHex,
                    topic0: step.topic0Hex,
                    topics: step.raw.topicsHex,
                    data: step.raw.dataHex,
                  },
                  null,
                  2
                )}
              </pre>
            </details>
          </div>
        ) : null}
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

function stringifyWithBigInt(value: unknown): string {
  return JSON.stringify(
    value,
    (_key, item: unknown) => (typeof item === "bigint" ? item.toString() : item),
    2
  );
}

function formatGasLimitAndUsage(gasLimit: bigint | null, gasUsed: bigint | null): string {
  if (gasLimit === null || gasUsed === null) {
    return "N/A";
  }
  const bps = calcRoundedBps(gasUsed, gasLimit);
  if (bps === null) {
    return `${formatInteger(gasUsed)} / ${formatInteger(gasLimit)}`;
  }
  const whole = bps / 100n;
  const fraction = (bps % 100n).toString().padStart(2, "0");
  return `${formatInteger(gasUsed)} / ${formatInteger(gasLimit)} (${whole.toString()}.${fraction}%)`;
}

function formatOptionalGwei(value: bigint | null): string {
  return value === null ? "N/A" : formatGweiWithComma(value);
}

function formatInteger(value: bigint): string {
  return value.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

function formatGweiWithComma(value: bigint): string {
  const text = formatGweiFromWei(value);
  const [numeric, unit] = text.split(" ");
  if (!numeric || !unit) {
    return text;
  }
  const sign = numeric.startsWith("-") ? "-" : "";
  const unsigned = sign ? numeric.slice(1) : numeric;
  const [whole, fraction] = unsigned.split(".");
  if (!whole) {
    return text;
  }
  const wholeWithComma = whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  const numericWithComma = fraction ? `${sign}${wholeWithComma}.${fraction}` : `${sign}${wholeWithComma}`;
  return `${numericWithComma} ${unit}`;
}

function formatTopicLabel(topic0Hex: string | null, index: number): string {
  const labels = topic0Hex ? ADDRESS_TOPIC_LABELS_BY_TOPIC0[topic0Hex] : null;
  if (labels && labels[index]) {
    return labels[index];
  }
  return `topic[${index.toString()}]`;
}

function formatEventName(step: TimelineStep): string {
  const topic0 = step.topic0Hex;
  if (topic0) {
    const known = EVENT_NAME_BY_TOPIC0[topic0];
    if (known) {
      return known;
    }
  }
  if (step.type === "unknown") {
    return "Unknown";
  }
  return "-";
}

const EVENT_NAME_BY_TOPIC0: Record<string, string> = {
  // ERC-20
  "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef": "Transfer",
  "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925": "Approval",

  // Uniswap V2
  "0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822": "Swap (Uniswap V2)",
  "0x4c209b5fc8ad50758f13e2e1088ba56a560dff690a1c6fef26394f4c03821c4f": "Mint (Uniswap V2)",
  "0xdccd412f0b1252819cb1fd330b93224ca42612892bb3f4f789976e6d81936496": "Burn (Uniswap V2)",
  "0x1c411e9a96e071241c2f21f7726b17ae89e3cab4c78be50e062b03a9fffbbad1": "Sync (Uniswap V2)",

  // Uniswap V3
  "0xc42079f94a6350d7e6235f29174924f928cc2ac818eb64fed8004e115fbcca67": "Swap (Uniswap V3)",

  // Aave
  "0x631042c832b07452973831137f2d73e395028b44b250dedc5abb0ee766e168ac": "FlashLoan (Aave V2)",
  "0xefefaba5e921573100900a3ad9cf29f222d995fb3b6045797eaea7521bd8d6f0": "FlashLoan (Aave V3)",
  "0xf164a7d9b7e450d8229718aed20376118864bcc756709e0fc1d0891133dd2fe8": "FlashLoanSimple (Aave V3)",

  // Common ownership/admin patterns
  "0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0": "OwnershipTransferred",
};

const ADDRESS_TOPIC_LABELS_BY_TOPIC0: Record<string, Record<number, string>> = {
  // event Transfer(address indexed from, address indexed to, uint256 value)
  "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef": {
    1: "from",
    2: "to",
  },
  // event Approval(address indexed owner, address indexed spender, uint256 value)
  "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925": {
    1: "owner",
    2: "spender",
  },
};

function renderTopicValue(topic0Hex: string | null, index: number, topicHex: string): ReactNode {
  const labels = topic0Hex ? ADDRESS_TOPIC_LABELS_BY_TOPIC0[topic0Hex] : null;
  const expectsAddress = Boolean(labels && labels[index]);
  if (!expectsAddress) {
    return topicHex;
  }
  const addressHex = decodeIndexedAddressTopic(topicHex);
  if (!addressHex) {
    return topicHex;
  }
  return (
    <Link href={`/address/${addressHex}`} className="text-sky-700 hover:underline">
      {addressHex}
    </Link>
  );
}

function decodeIndexedAddressTopic(topicHex: string): string | null {
  if (!/^0x[0-9a-fA-F]{64}$/.test(topicHex)) {
    return null;
  }
  return `0x${topicHex.slice(-40).toLowerCase()}`;
}
