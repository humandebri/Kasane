"use client";

// どこで: App provider集約 / 何を: wallet contextをアプリ全体へ適用 / なぜ: 全画面で接続状態を利用するため

import type { ReactNode } from "react";
import { WalletProvider } from "@/lib/wallet/provider";

export function Providers({ children }: { children: ReactNode }) {
  return <WalletProvider>{children}</WalletProvider>;
}
