"use client";

// どこで: wallet provider / 何を: 接続状態とconnect/disconnect操作をContextで提供 / なぜ: 画面全体でII/Oisy状態を共有するため

import { createContext, useCallback, useMemo, useState, type ReactNode } from "react";
import { connectInternetIdentity, disconnectInternetIdentity } from "./ii";
import { connectOisy, disconnectOisy, isOisyAvailable } from "./oisy";
import type { WalletSession, WalletSource } from "./types";

type WalletContextValue = {
  session: WalletSession | null;
  connecting: boolean;
  error: string | null;
  oisyAvailable: boolean;
  connect: (source: WalletSource) => Promise<void>;
  disconnect: () => Promise<void>;
  clearError: () => void;
};

export const WalletContext = createContext<WalletContextValue | null>(null);

export function WalletProvider(
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
  const [session, setSession] = useState<WalletSession | null>(null);
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [oisyAvailable] = useState<boolean>(() => isOisyAvailable());

  const connect = useCallback(async (source: WalletSource) => {
    setConnecting(true);
    setError(null);
    try {
      const nextSession = source === "ii"
        ? await connectInternetIdentity(iiIdentityProvider, iiDerivationOrigin)
        : await connectOisy();
      setSession(nextSession);
    } catch (e) {
      const message = e instanceof Error ? e.message : "wallet.connect_failed";
      setError(message);
    } finally {
      setConnecting(false);
    }
  }, [iiDerivationOrigin, iiIdentityProvider]);

  const disconnect = useCallback(async () => {
    const current = session;
    setSession(null);
    setError(null);
    if (!current) {
      return;
    }
    if (current.source === "ii") {
      await disconnectInternetIdentity();
      return;
    }
    await disconnectOisy();
  }, [session]);

  const clearError = useCallback(() => {
    setError(null);
  }, []);

  const value = useMemo<WalletContextValue>(() => ({
    session,
    connecting,
    error,
    oisyAvailable,
    connect,
    disconnect,
    clearError,
  }), [session, connecting, error, oisyAvailable, connect, disconnect, clearError]);

  return <WalletContext.Provider value={value}>{children}</WalletContext.Provider>;
}
