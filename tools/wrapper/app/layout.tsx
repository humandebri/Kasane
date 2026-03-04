// どこで: App root layout / 何を: 共通メタ情報とグローバルCSSを適用 / なぜ: 全ページの表示基盤を揃えるため

import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Wrapper Dashboard",
  description: "Kasane unwrap submit and wrap recovery dashboard",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="ja">
      <body>{children}</body>
    </html>
  );
}
