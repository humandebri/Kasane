// どこで: App root layout / 何を: 共通メタ情報・グローバルCSS・providerを適用 / なぜ: wallet接続を全画面で共有するため

import type { Metadata } from "next";
import { Providers } from "./providers";
import "./globals.css";

export const metadata: Metadata = {
  title: "Wrapper Dashboard",
  description: "Kasane unwrap submit and wrap recovery dashboard",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="ja">
      <body>
        <Providers>{children}</Providers>
      </body>
    </html>
  );
}
