// どこで: wallet 接続型定義 / 何を: Oisy と MetaMask の接続状態を定義 / なぜ: canister signer と EVM sender の責務を分離するため

export type WalletSource = "oisy";

export type WalletSession = {
  principalText: string;
  source: WalletSource;
};

export type OisyCapabilities = {
  ledgerApproveSupported: boolean;
  wrapCanisterSupported: boolean;
};

export type MetaMaskSession = {
  accountAddress: string;
  chainIdHex: string;
};
