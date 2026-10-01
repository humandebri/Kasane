// どこで: 404ページ / 何を: 探索導線を維持したエラー表示 / なぜ: 誤入力時に復帰しやすくするため

import { Link } from "@tanstack/react-router";
import { Card, CardContent, CardHeader, CardTitle } from "./ui/card";

export default function NotFound() {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Not Found</CardTitle>
      </CardHeader>
      <CardContent className="space-y-2 text-sm">
        <p>対象データが見つかりませんでした。</p>
        <Link to="/" className="text-sky-700 hover:underline">
          Back to Home
        </Link>
      </CardContent>
    </Card>
  );
}
