"use client";

// どこで: App provider集約 / 何を: wallet contextをアプリ全体へ適用 / なぜ: 全画面で接続状態を利用するため

import type { ReactNode } from "react";
import { WalletProvider } from "@/lib/wallet/provider";

export function Providers(
  {
    children,
    iiIdentityProvider,
    iiDerivationOrigin,
  }: {
    children: ReactNode;
    iiIdentityProvider: string | null;
    iiDerivationOrigin: string | null;
  },
) {
  return (
    <WalletProvider
      iiIdentityProvider={iiIdentityProvider}
      iiDerivationOrigin={iiDerivationOrigin}
    >
      {children}
    </WalletProvider>
  );
}
