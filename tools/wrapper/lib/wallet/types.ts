// どこで: wallet接続型定義 / 何を: II/Oisy接続の共通契約を定義 / なぜ: UI層と接続実装の責務を分離するため

import type { Identity } from "@dfinity/agent";

export type WalletSource = "ii" | "oisy";

export type WalletSession = {
  identity: Identity;
  principalText: string;
  source: WalletSource;
};

type OisyConnectResult = {
  identity?: Identity;
};

export type OisyProvider = {
  connect?: (options?: Record<string, unknown>) => Promise<OisyConnectResult | void>;
  disconnect?: () => Promise<void>;
  getIdentity?: () => Promise<Identity>;
};
