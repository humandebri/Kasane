// どこで: Oisy接続アダプタ / 何を: ブラウザ注入providerからidentity取得を試行 / なぜ: Oisy接続をUIに統合するため

import type { Identity } from "@dfinity/agent";
import type { OisyProvider, WalletSession } from "./types";

function getProvider(): OisyProvider | null {
  if (typeof window === "undefined") {
    return null;
  }
  if (window.oisy) {
    return window.oisy;
  }
  if (window.ic?.oisy) {
    return window.ic.oisy;
  }
  return null;
}

function isIdentity(value: unknown): value is Identity {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  if (!("getPrincipal" in value)) {
    return false;
  }
  return typeof value.getPrincipal === "function";
}

export function isOisyAvailable(): boolean {
  return getProvider() !== null;
}

export async function connectOisy(): Promise<WalletSession> {
  const provider = getProvider();
  if (!provider) {
    throw new Error("wallet.oisy_unavailable");
  }

  const connectResult = provider.connect ? await provider.connect() : undefined;
  const directIdentity = connectResult?.identity;
  const providerIdentity = provider.getIdentity ? await provider.getIdentity() : undefined;
  const identity = providerIdentity ?? directIdentity;

  if (!isIdentity(identity)) {
    throw new Error("wallet.oisy_identity_missing");
  }

  return {
    identity,
    principalText: identity.getPrincipal().toText(),
    source: "oisy",
  };
}

export async function disconnectOisy(): Promise<void> {
  const provider = getProvider();
  if (!provider || !provider.disconnect) {
    return;
  }
  await provider.disconnect();
}
