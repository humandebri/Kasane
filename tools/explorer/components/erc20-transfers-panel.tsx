// どこで: Tx詳細のERC-20セクション / 何を: transferをFrom/To/For形式で表示 / なぜ: Etherscan風に可読性を上げるため

import Link from "next/link";
import { shortHex } from "../lib/hex";

export type Erc20TransferRowView = {
  logIndex: number;
  tokenAddressHex: string;
  tokenSymbol: string | null;
  fromAddressHex: string;
  toAddressHex: string;
  amountText: string;
  isRawAmount: boolean;
  kind: "mint" | "burn" | "transfer";
};

const ZERO_ADDRESS_HEX = `0x${"0".repeat(40)}`;

export function Erc20TransfersPanel({ transfers }: { transfers: Erc20TransferRowView[] }) {
  return (
    <section >
      <TransferSection rows={transfers} />
    </section>
  );
}

function TransferSection({
  rows,
}: {
  rows: Erc20TransferRowView[];
}) {
  return (
    <div className="space-y-2">
      {rows.length === 0 ? (
        <p className=" ">No transfers.</p>
      ) : (
        <div className="space-y-2">
          {rows.map((row, index) => (
            <article
              key={`${row.logIndex}:${row.tokenAddressHex}:${row.fromAddressHex}:${row.toAddressHex}:${index.toString()}`}
            >
              {row.kind === "mint" ? <MintTransferRow row={row} /> : null}
              {row.kind === "burn" ? <BurnTransferRow row={row} /> : null}
              {row.kind === "transfer" ? <StandardTransferRow row={row} /> : null}
            </article>
          ))}
        </div>
      )}
    </div>
  );
}

function MintTransferRow({ row }: { row: Erc20TransferRowView }) {
  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-slate-900">
      <span className="font-semibold">Mint</span>
      <span>to</span>
      <AddressLink addressHex={row.toAddressHex} />
      <span className="font-semibold">For</span>
      <AmountToken row={row} />
    </div>
  );
}

function BurnTransferRow({ row }: { row: Erc20TransferRowView }) {
  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-slate-900">
      <span className="font-semibold">Burn</span>
      <span>from</span>
      <AddressLink addressHex={row.fromAddressHex} />
      <span className="font-semibold">For</span>
      <AmountToken row={row} />
    </div>
  );
}

function StandardTransferRow({ row }: { row: Erc20TransferRowView }) {
  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-slate-900">
      <span className="font-semibold">From</span>
      <AddressLink addressHex={row.fromAddressHex} />
      <span className="font-semibold">To</span>
      <AddressLink addressHex={row.toAddressHex} />
      <span className="font-semibold">For</span>
      <AmountToken row={row} />
    </div>
  );
}

function AmountToken({ row }: { row: Erc20TransferRowView }) {
  return (
    <span className="font-mono">
      {row.amountText}
      {" "}
      <TokenLink tokenAddressHex={row.tokenAddressHex} tokenSymbol={row.tokenSymbol} />
      {row.isRawAmount ? " (raw)" : ""}
    </span>
  );
}

function TokenLink({ tokenAddressHex, tokenSymbol }: { tokenAddressHex: string; tokenSymbol: string | null }) {
  return (
    <Link href={`/address/${tokenAddressHex}`} className="inline-flex items-center gap-1 text-sky-700 hover:underline">
      <span>{tokenSymbol ?? shortHex(tokenAddressHex, 10)}</span>
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
