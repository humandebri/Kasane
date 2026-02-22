// どこで: Tx一覧系UI / 何を: Directionラベルを色付きバッジで表示 / なぜ: in/out/selfを視認しやすく統一するため

import { Badge } from "./ui/badge";

type TxDirection = "in" | "out" | "self";

export function TxDirectionBadge({ direction }: { direction: TxDirection }) {
  const className =
    direction === "in"
      ? "border-emerald-200 bg-emerald-50 text-emerald-700"
      : direction === "out"
        ? "border-orange-200 bg-orange-50 text-orange-700"
        : "border-slate-200 bg-slate-50 text-slate-700";

  return (
    <Badge variant="outline" className={className}>
      {direction}
    </Badge>
  );
}
