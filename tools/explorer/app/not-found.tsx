// どこで: 404ページ / 何を: 探索導線を維持したエラー表示 / なぜ: 誤入力時に復帰しやすくするため

import Link from "next/link";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";

export default function NotFound() {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Not Found</CardTitle>
      </CardHeader>
      <CardContent className="space-y-2 text-sm">
        <p>対象データが見つかりませんでした。</p>
        <Link href="/" className="text-sky-700 hover:underline">
          Back to Home
        </Link>
      </CardContent>
    </Card>
  );
}
