// どこで: ルートレイアウト / 何を: 共通ヘッダーと枠組みを提供 / なぜ: ページ間で導線と情報密度を揃えるため

import type { Metadata } from "next";
import Link from "next/link";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Badge } from "../components/ui/badge";
import "./globals.css";

export const metadata: Metadata = {
  title: "Kasane Explorer",
  description: "Postgres-backed operations explorer for Kasane",
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="ja">
      <body>
        <main className="mx-auto grid w-full max-w-6xl gap-4 p-4 md:p-6">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-xl">
                Kasane Explorer <Badge variant="secondary">Phase2.1</Badge>
              </CardTitle>
            </CardHeader>
            <CardContent className="flex items-center gap-4 text-sm">
              <Link href="/" className="text-sky-700 hover:underline">
                Home
              </Link>
            </CardContent>
          </Card>
          {children}
        </main>
      </body>
    </html>
  );
}
