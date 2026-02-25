// どこで: Explorer共通UI / 何を: tx hashリンクと失敗アイコン表示を共通化 / なぜ: 画面ごとの差分漏れを防ぎ一貫表示にするため

import Link from "next/link";
import { AlertTriangle } from "lucide-react";
import type { ReactNode } from "react";

type Props = {
  txHashHex: string;
  receiptStatus: number | null;
  children?: ReactNode;
  className?: string;
  title?: string;
};

export function TxHashLink({
  txHashHex,
  receiptStatus,
  children,
  className = "text-sky-700 hover:underline",
  title,
}: Props) {
  return (
    <span className="inline-flex items-center gap-1">
      {receiptStatus === 0 ? (
        <span title="Failed transaction">
          <AlertTriangle className="h-3.5 w-3.5 text-rose-600" aria-label="failed transaction" />
        </span>
      ) : null}
      <Link href={`/tx/${txHashHex}`} className={className} title={title}>
        {children ?? txHashHex}
      </Link>
    </span>
  );
}
