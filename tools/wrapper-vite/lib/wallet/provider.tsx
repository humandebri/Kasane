"use client";

// どこで: wallet provider / 何を: Juno auth の接続状態と認証操作を Context で提供 / なぜ: 認証導線を Google / II の 1 系統に統一するため

import { createContext, useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  getIdentityOnce,
  initSatellite,
  onAuthStateChange,
  signIn,
  signOut,
  type InternetIdentityDomain,
  type User,
} from "@junobuild/core";
import type { Identity } from "@icp-sdk/core/agent";
import type { WalletSession, WalletSource } from "./types";

const GOOGLE_RETURN_TO_STORAGE_KEY = "wrapper-vite:google-return-to";

type WalletContextValue = {
  session: WalletSession | null;
  connecting: boolean;
  error: string | null;
  connectGoogle: () => Promise<void>;
  connectInternetIdentity: () => Promise<void>;
  disconnect: () => Promise<void>;
  clearError: () => void;
  getIdentity: () => Promise<Identity | null>;
};

function mapUserToSession(user: User | null): WalletSession | null {
  if (!user || user.owner === undefined) {
    return null;
  }
  const source: WalletSource =
    user.data.provider === "google"
      ? "google"
      : "ii";
  return {
    principalText: user.owner,
    source,
  };
}

export const WalletContext = createContext<WalletContextValue | null>(null);

function saveGoogleReturnToPath(path: string): void {
  if (typeof globalThis.sessionStorage === "undefined") {
    return;
  }
  if (!path.startsWith("/") || path.startsWith("/auth/callback")) {
    return;
  }
  globalThis.sessionStorage.setItem(GOOGLE_RETURN_TO_STORAGE_KEY, path);
}

function buildGoogleSignInOptions(googleClientId: string | null): {
  google: {
    options: {
      redirect: {
        clientId: string | undefined;
        redirectUrl: string;
      };
    };
  };
} {
  return {
    google: {
      options: {
        redirect: {
          clientId: googleClientId ?? undefined,
          redirectUrl: new URL("/auth/callback", globalThis.location.origin).toString(),
        },
      },
    },
  };
}

export function WalletProvider(
  {
    children,
    satelliteId,
    googleClientId,
    iiDomain,
    iiDerivationOrigin,
  }: {
    children: ReactNode;
    satelliteId: string | null;
    googleClientId: string | null;
    iiDomain: InternetIdentityDomain | null;
    iiDerivationOrigin: string | null;
  },
) {
  const [session, setSession] = useState<WalletSession | null>(null);
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    const unsubs: Array<() => void> = [];
    void initSatellite(satelliteId ? { satelliteId } : undefined)
      .then((initUnsubs) => {
        if (!mounted) {
          initUnsubs.forEach((unsubscribe) => unsubscribe());
          return;
        }
        unsubs.push(...initUnsubs);
        const unsubscribe = onAuthStateChange((authUser) => {
          if (!mounted) {
            return;
          }
          setSession(mapUserToSession(authUser));
          setConnecting(false);
        });
        unsubs.push(unsubscribe);
      })
      .catch((nextError) => {
        if (!mounted) {
          return;
        }
        setError(nextError instanceof Error ? nextError.message : "wallet.init_failed");
      });
    return () => {
      mounted = false;
      for (const unsubscribe of unsubs) {
        unsubscribe();
      }
    };
  }, [satelliteId]);

  const connectGoogle = useCallback(async () => {
    setConnecting(true);
    setError(null);
    try {
      saveGoogleReturnToPath(
        `${globalThis.location.pathname}${globalThis.location.search}${globalThis.location.hash}`,
      );
      await signIn(buildGoogleSignInOptions(googleClientId));
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : "wallet.connect_failed");
      setConnecting(false);
    }
  }, [googleClientId]);

  const connectInternetIdentity = useCallback(async () => {
    setConnecting(true);
    setError(null);
    try {
      await signIn({
        internet_identity: {
          options: {
            domain: iiDomain ?? undefined,
            derivationOrigin: iiDerivationOrigin ?? undefined,
            maxTimeToLiveInNanoseconds: 24n * 60n * 60n * 1_000_000_000n,
          },
        },
      });
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : "wallet.connect_failed");
    } finally {
      setConnecting(false);
    }
  }, [iiDerivationOrigin, iiDomain]);

  const disconnect = useCallback(async () => {
    setError(null);
    setSession(null);
    await signOut({ windowReload: false });
  }, []);

  const clearError = useCallback(() => {
    setError(null);
  }, []);

  const getIdentity = useCallback(async (): Promise<Identity | null> => {
    return getIdentityOnce();
  }, []);

  const value = useMemo<WalletContextValue>(() => ({
    session,
    connecting,
    error,
    connectGoogle,
    connectInternetIdentity,
    disconnect,
    clearError,
    getIdentity,
  }), [
    session,
    connecting,
    error,
    connectGoogle,
    connectInternetIdentity,
    disconnect,
    clearError,
    getIdentity,
  ]);

  return <WalletContext.Provider value={value}>{children}</WalletContext.Provider>;
}

export const walletProviderTestHooks = {
  GOOGLE_RETURN_TO_STORAGE_KEY,
  saveGoogleReturnToPath,
  buildGoogleSignInOptions,
};
