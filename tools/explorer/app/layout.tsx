// どこで: ルートレイアウト / 何を: 共通ナビと検索導線を提供 / なぜ: 全ページで同じ導線と情報密度を保つため

import type { Metadata } from "next";
import { AppHeader } from "../components/app-header";
import "./globals.css";

export const metadata: Metadata = {
  title: "Kasane Explorer",
  description: "Postgres-backed operations explorer for Kasane",
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="ja">
      <body>
        <main className="relative mx-auto box-border min-h-dvh w-full max-w-[96rem] space-y-4 px-4 pb-8 pt-5 sm:px-6">
          <AppHeader />

          <section className="space-y-4">{children}</section>
        </main>
      </body>
    </html>
  );
}
