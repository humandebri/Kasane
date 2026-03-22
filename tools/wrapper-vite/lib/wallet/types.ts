// どこで: wallet 接続型定義 / 何を: Juno auth ベースの認証状態を定義 / なぜ: UI と認証実装の責務を分離するため

export type WalletSource = "google" | "ii";

export type WalletSession = {
  principalText: string;
  source: WalletSource;
};
