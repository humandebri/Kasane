// どこで: II接続アダプタ / 何を: Internet Identityログインとidentity取得を提供 / なぜ: ウォレット署名でupdate callを実行するため

import { AuthClient } from "@dfinity/auth-client";
import type { WalletSession } from "./types";

let cachedAuthClient: AuthClient | null = null;
const DEFAULT_MAINNET_IDENTITY_PROVIDER = "https://identity.ic0.app";

async function getAuthClient(): Promise<AuthClient> {
  if (cachedAuthClient) {
    return cachedAuthClient;
  }
  cachedAuthClient = await AuthClient.create();
  return cachedAuthClient;
}

function resolveIdentityProvider(configured: string | null): string {
  return configured && configured.trim() !== ""
    ? configured.trim()
    : DEFAULT_MAINNET_IDENTITY_PROVIDER;
}

function resolveDerivationOrigin(configured: string | null): string | undefined {
  if (!configured) {
    return undefined;
  }
  const trimmed = configured.trim();
  return trimmed === "" ? undefined : trimmed;
}

export async function connectInternetIdentity(
  identityProvider: string | null,
  derivationOrigin: string | null,
): Promise<WalletSession> {
  const authClient = await getAuthClient();
  const authenticated = await authClient.isAuthenticated();

  if (!authenticated) {
    await new Promise<void>((resolve, reject) => {
      authClient.login({
        identityProvider: resolveIdentityProvider(identityProvider),
        derivationOrigin: resolveDerivationOrigin(derivationOrigin),
        maxTimeToLive: 10n * 60n * 60n * 1_000_000_000n,
        onSuccess: () => resolve(),
        onError: (message?: string) => reject(new Error(`wallet.ii_login_failed:${message ?? "unknown"}`)),
      });
    });
  }

  const identity = authClient.getIdentity();
  const principalText = identity.getPrincipal().toText();
  return {
    identity,
    principalText,
    source: "ii",
  };
}

export async function disconnectInternetIdentity(): Promise<void> {
  const authClient = await getAuthClient();
  await authClient.logout();
}

export const iiTestHooks = {
  resolveIdentityProvider,
  resolveDerivationOrigin,
};
