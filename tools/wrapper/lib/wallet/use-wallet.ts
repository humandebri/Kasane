"use client";

// どこで: wallet hook / 何を: WalletContextを安全に参照する / なぜ: provider外参照の実行時バグを防ぐため

import { useContext } from "react";
import { WalletContext } from "./provider";

export function useWallet() {
  const value = useContext(WalletContext);
  if (!value) {
    throw new Error("wallet.provider_missing");
  }
  return value;
}
