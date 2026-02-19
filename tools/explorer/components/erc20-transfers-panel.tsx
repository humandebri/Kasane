// どこで: Tx詳細のERC-20セクション / 何を: transferをFrom/To/For形式で表示 / なぜ: Etherscan風に可読性を上げるため

import Link from "next/link";
import type { TxDetailView } from "../lib/data";
import { formatTokenAmount } from "../lib/format";
import { shortHex } from "../lib/hex";

type TransferItem = TxDetailView["erc20Transfers"][number];

const ZERO_ADDRESS_HEX = `0x${"0".repeat(40)}`;

export function Erc20TransfersPanel({ transfers }: { transfers: TransferItem[] }) {
  return (
    <section className="space-y-3 rounded-md border border-slate-200 p-3">
      <div className="space-y-2">
        <div className="text-sm font-semibold">{`ERC-20 Tokens Transferred:${transfers.length.toString()}`}</div>
        <div className="inline-flex overflow-hidden rounded-md border border-slate-300 text-xs">
          <span className="bg-gray-100 px-3 py-1.5 font-medium text-slate-700">All Transfers</span>
        </div>
      </div>

      <TransferSection title="All Transfers" rows={transfers} />
    </section>
  );
}

function TransferSection({
  title,
  rows,
}: {
  title: string;
  rows: TransferItem[];
}) {
  return (
    <div className="space-y-2">
      <div className="text-xs font-medium text-slate-600">{title}</div>
      {rows.length === 0 ? (
        <p className="text-xs text-slate-500">No transfers.</p>
      ) : (
        <div className="space-y-2">
          {rows.map((row, index) => (
            <article
              key={`${row.logIndex}:${row.tokenAddressHex}:${row.fromAddressHex}:${row.toAddressHex}:${index.toString()}`}
              className="rounded-md border border-slate-200 bg-slate-50 p-3"
            >
              <div className="flex flex-wrap items-center gap-2 text-xs">
                <TokenLink tokenAddressHex={row.tokenAddressHex} tokenSymbol={row.tokenSymbol} />
              </div>
              <dl className="mt-2 grid grid-cols-1 gap-y-1 text-xs md:grid-cols-[70px_1fr]">
                <dt className="text-slate-500">From</dt>
                <dd>
                  <AddressLink addressHex={row.fromAddressHex} />
                </dd>
                <dt className="text-slate-500">To</dt>
                <dd>
                  <AddressLink addressHex={row.toAddressHex} />
                </dd>
                <dt className="text-slate-500">For</dt>
                <dd className="font-mono text-slate-900">
                  {formatTokenAmount(row.amount, row.tokenDecimals)}
                  {row.tokenSymbol ? ` ${row.tokenSymbol}` : ""}
                  {row.tokenDecimals === null ? " (raw)" : ""}
                </dd>
              </dl>
            </article>
          ))}
        </div>
      )}
    </div>
  );
}

function TokenLink({ tokenAddressHex, tokenSymbol }: { tokenAddressHex: string; tokenSymbol: string | null }) {
  return (
    <Link href={`/address/${tokenAddressHex}`} className="inline-flex items-center gap-1 text-sky-700 hover:underline">
      <span>{tokenSymbol ?? shortHex(tokenAddressHex, 10)}</span>
      <span className="font-mono text-[11px] text-slate-500">{shortHex(tokenAddressHex, 8)}</span>
    </Link>
  );
}

function AddressLink({ addressHex }: { addressHex: string }) {
  const normalized = addressHex.toLowerCase();
  const isZeroAddress = normalized === ZERO_ADDRESS_HEX;
  const label = isZeroAddress ? `Null: ${shortHex(normalized, 6)}` : shortHex(normalized);
  return (
    <Link href={`/address/${normalized}`} className="font-mono text-sky-700 hover:underline">
      {label}
    </Link>
  );
}
