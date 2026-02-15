// どこで: アドレス詳細ページ / 何を: アドレスのスナップショット情報を表示 / なぜ: 公開導線として残高/nonce/コード有無を即確認できるようにするため

import { notFound } from "next/navigation";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { getAddressView } from "../../../lib/data";
import { isAddressHex, normalizeHex } from "../../../lib/hex";

export const dynamic = "force-dynamic";

export default async function AddressPage({ params }: { params: Promise<{ hex: string }> }) {
  const { hex } = await params;
  if (!isAddressHex(hex)) {
    notFound();
  }
  const data = await getAddressView(normalizeHex(hex));

  return (
    <Card>
      <CardHeader>
        <CardTitle>Address</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <dl className="grid grid-cols-1 gap-2 text-sm md:grid-cols-[180px_1fr]">
          <dt className="text-muted-foreground">Address</dt>
          <dd className="font-mono">{data.addressHex}</dd>
          <dt className="text-muted-foreground">Balance (wei)</dt>
          <dd className="font-mono">{data.balance === null ? "N/A" : data.balance.toString()}</dd>
          <dt className="text-muted-foreground">Nonce</dt>
          <dd>{data.nonce === null ? "N/A" : data.nonce.toString()}</dd>
          <dt className="text-muted-foreground">Code Bytes</dt>
          <dd>{data.codeBytes === null ? "N/A" : data.codeBytes.toString()}</dd>
          <dt className="text-muted-foreground">Type</dt>
          <dd>
            <Badge variant={data.isContract === true ? "secondary" : "outline"}>
              {data.isContract === null ? "Unknown" : data.isContract ? "Contract" : "EOA"}
            </Badge>
          </dd>
        </dl>

        <div className="rounded-md border bg-slate-50 p-3 text-sm">
          Address起点の履歴一覧は次フェーズ対応です（現行indexerには from/to 索引がありません）。
        </div>

        {data.warnings.length > 0 ? (
          <div className="rounded-md border bg-amber-50 p-3 text-sm">
            <div className="mb-1 font-medium">Warnings</div>
            <ul className="list-disc pl-5">
              {data.warnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          </div>
        ) : null}
      </CardContent>
    </Card>
  );
}
