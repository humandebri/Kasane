"use client";

// どこで: wallet provider
// 何を: Oisy signer と MetaMask の接続状態を Context で提供
// なぜ: canister signer と EVM sender を /swap 風 UI から一元制御するため

import { createContext, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { HttpAgent } from "@icp-sdk/core/agent";
import { Principal } from "@icp-sdk/core/principal";
import { Signer } from "@slide-computer/signer";
import { SignerAgent } from "@slide-computer/signer-agent";
import { PostMessageTransport } from "@slide-computer/signer-web";
import type { AuthenticatedCaller } from "@/lib/canister/authenticated-caller";
import { hasIcrc21Support } from "@/lib/canister/standards-client";
import { configTestHooks } from "@/lib/config";
import { OISY_SIGNER_URL } from "./oisy";
import {
  ensureMetaMaskChain,
  getMetaMaskAccounts,
  getMetaMaskChainId,
  getMetaMaskProvider,
  normalizeMetaMaskAddress,
  parseMetaMaskAccountsChanged,
  parseMetaMaskChainChanged,
  unknownToErrorMessage,
  type MetaMaskChainConfig,
} from "./metamask";
import type { MetaMaskSession, WalletSession } from "./types";

type OisyCapabilityState = {
  ledgerApproveSupported: boolean;
  wrapCanisterSupported: boolean;
  gatewaySupported: boolean;
};

type WalletContextValue = {
  oisySession: WalletSession | null;
  metaMaskSession: MetaMaskSession | null;
  oisyConnecting: boolean;
  metaMaskConnecting: boolean;
  metaMaskAvailable: boolean;
  oisyCapabilities: OisyCapabilityState;
  error: string | null;
  connectOisy: () => Promise<void>;
  connectMetaMask: () => Promise<void>;
  disconnectOisy: () => Promise<void>;
  disconnectMetaMask: () => void;
  clearError: () => void;
  getCaller: () => Promise<AuthenticatedCaller | null>;
};

function resolveBoundFetch(): typeof globalThis.fetch | undefined {
  if (typeof globalThis.fetch !== "function") {
    return undefined;
  }
  return globalThis.fetch.bind(globalThis);
}

function mapPrincipalToSession(principalText: string | null): WalletSession | null {
  if (principalText === null) {
    return null;
  }
  return { principalText, source: "oisy" };
}

type OisyAccountList = Awaited<ReturnType<Signer["accounts"]>>;
type OisyAccountOwner = {
  isAnonymous: () => boolean;
  toText: () => string;
};
type OisyAccountLike = {
  owner: OisyAccountOwner;
};

async function resolveMetaMaskSession(chainConfig: MetaMaskChainConfig): Promise<MetaMaskSession | null> {
  const provider = getMetaMaskProvider();
  if (provider === null) {
    return null;
  }
  const accounts = await getMetaMaskAccounts(provider);
  const firstAccount = accounts[0];
  if (firstAccount === undefined) {
    return null;
  }
  const chainIdHex = await getMetaMaskChainId(provider);
  return {
    accountAddress: normalizeMetaMaskAddress(firstAccount),
    chainIdHex: chainIdHex.toLowerCase() !== `0x${chainConfig.chainId.toString(16)}`.toLowerCase()
      ? chainIdHex
      : chainIdHex,
  };
}

function createOisySigner(derivationOrigin: string | null): Signer {
  return new Signer({
    transport: new PostMessageTransport({
      url: OISY_SIGNER_URL,
      windowOpenerFeatures: "width=525,height=705",
      establishTimeout: 45_000,
      disconnectTimeout: 45_000,
      detectNonClickEstablishment: false,
    }),
    derivationOrigin: derivationOrigin ?? undefined,
  });
}

async function createOisyBaseAgent(icHost: string): Promise<HttpAgent> {
  const agent = new HttpAgent({ host: icHost, fetch: resolveBoundFetch() });
  if (configTestHooks.shouldFetchRootKey(icHost)) {
    await agent.fetchRootKey();
  }
  return agent;
}

function resolveOisyPrincipalText(accounts: ReadonlyArray<OisyAccountLike>): string | null {
  const firstAccount = accounts[0];
  if (firstAccount === undefined || firstAccount.owner.isAnonymous()) {
    return null;
  }
  return firstAccount.owner.toText();
}

export const WalletContext = createContext<WalletContextValue | null>(null);

export function WalletProvider(
  {
    children,
    icHost,
    oisyDerivationOrigin,
    wrapCanisterId,
    evmCanisterId,
    metaMaskChain,
  }: {
    children: ReactNode;
    icHost: string;
    oisyDerivationOrigin: string | null;
    wrapCanisterId: string | null;
    evmCanisterId: string | null;
    metaMaskChain: MetaMaskChainConfig;
  },
) {
  const [oisySession, setOisySession] = useState<WalletSession | null>(null);
  const [metaMaskSession, setMetaMaskSession] = useState<MetaMaskSession | null>(null);
  const [oisyConnecting, setOisyConnecting] = useState(false);
  const [metaMaskConnecting, setMetaMaskConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [metaMaskAvailable, setMetaMaskAvailable] = useState(getMetaMaskProvider() !== null);
  const [oisyCapabilities, setOisyCapabilities] = useState<OisyCapabilityState>({
    ledgerApproveSupported: true,
    wrapCanisterSupported: false,
    gatewaySupported: false,
  });
  const callerRef = useRef<AuthenticatedCaller | null>(null);
  const signerRef = useRef<Signer | null>(null);
  const oisySignerSessionRef = useRef(0);

  const resetOisyConnectionState = useCallback(async (): Promise<void> => {
    callerRef.current = null;
    setOisySession(null);
    setOisyCapabilities({
      ledgerApproveSupported: true,
      wrapCanisterSupported: false,
      gatewaySupported: false,
    });
    const signer = signerRef.current;
    signerRef.current = null;
    await signer?.closeChannel();
  }, []);

  const probeCapabilities = useCallback(async (): Promise<void> => {
    const [wrapSupported, gatewaySupported] = await Promise.all([
      hasIcrc21Support(wrapCanisterId),
      hasIcrc21Support(evmCanisterId),
    ]);
    setOisyCapabilities({
      ledgerApproveSupported: true,
      wrapCanisterSupported: wrapSupported,
      gatewaySupported,
    });
  }, [evmCanisterId, wrapCanisterId]);

  const connectToOisy = useCallback(async (reportErrors: boolean): Promise<void> => {
    try {
      const baseAgent = await createOisyBaseAgent(icHost);
      const signer = createOisySigner(oisyDerivationOrigin);
      const accounts = await signer.accounts();
      const principalText = resolveOisyPrincipalText(accounts);
      if (principalText === null) {
        await signer.closeChannel();
        throw new Error("wallet.oisy_account_missing");
      }
      const signerAgent = await SignerAgent.create({
        signer,
        account: Principal.fromText(principalText),
        agent: baseAgent,
      });
      const previousSigner = signerRef.current;
      oisySignerSessionRef.current += 1;
      signerRef.current = signer;
      callerRef.current = {
        principalText,
        cacheKey: `${principalText}:oisy:${oisySignerSessionRef.current}`,
        agent: signerAgent,
      };
      setOisySession(mapPrincipalToSession(principalText));
      await probeCapabilities();
      await previousSigner?.closeChannel();
    } catch (nextError) {
      await resetOisyConnectionState();
      if (reportErrors) {
        setError(unknownToErrorMessage(nextError, "wallet.oisy_connect_failed"));
      }
    }
  }, [icHost, oisyDerivationOrigin, probeCapabilities, resetOisyConnectionState]);

  useEffect(() => {
    const provider = getMetaMaskProvider();
    setMetaMaskAvailable(provider !== null);
    if (provider === null) {
      setMetaMaskSession(null);
      return;
    }

    const syncMetaMask = async (): Promise<void> => {
      try {
        setMetaMaskSession(await resolveMetaMaskSession(metaMaskChain));
      } catch (nextError) {
        setError(unknownToErrorMessage(nextError, "wallet.metamask_sync_failed"));
      }
    };

    void syncMetaMask();

    const handleAccountsChanged = (value: unknown): void => {
      const accounts = parseMetaMaskAccountsChanged(value);
      const firstAccount = accounts[0];
      if (firstAccount === undefined) {
        setMetaMaskSession(null);
        return;
      }
      setMetaMaskSession((current) => ({
        accountAddress: normalizeMetaMaskAddress(firstAccount),
        chainIdHex: current?.chainIdHex ?? `0x${metaMaskChain.chainId.toString(16)}`,
      }));
    };

    const handleChainChanged = (value: unknown): void => {
      const chainIdHex = parseMetaMaskChainChanged(value);
      if (chainIdHex === null) {
        return;
      }
      setMetaMaskSession((current) => (current === null ? null : { ...current, chainIdHex }));
    };

    provider.on("accountsChanged", handleAccountsChanged);
    provider.on("chainChanged", handleChainChanged);
    return () => {
      provider.removeListener("accountsChanged", handleAccountsChanged);
      provider.removeListener("chainChanged", handleChainChanged);
    };
  }, [metaMaskChain]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const baseAgent = await createOisyBaseAgent(icHost).catch(() => null);
      if (baseAgent === null || cancelled) {
        return;
      }
      const signer = createOisySigner(oisyDerivationOrigin);
      try {
        const accounts = await signer.accounts();
        const principalText = resolveOisyPrincipalText(accounts);
        if (cancelled) {
          await signer.closeChannel();
          return;
        }
        if (principalText === null) {
          await signer.closeChannel();
          return;
        }
        const signerAgent = await SignerAgent.create({
          signer,
          account: Principal.fromText(principalText),
          agent: baseAgent,
        });
        if (cancelled) {
          await signer.closeChannel();
          return;
        }
        const previousSigner = signerRef.current;
        oisySignerSessionRef.current += 1;
        signerRef.current = signer;
        callerRef.current = {
          principalText,
          cacheKey: `${principalText}:oisy:${oisySignerSessionRef.current}`,
          agent: signerAgent,
        };
        setOisySession(mapPrincipalToSession(principalText));
        await probeCapabilities();
        await previousSigner?.closeChannel();
      } catch {
        await signer.closeChannel();
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [icHost, oisyDerivationOrigin, probeCapabilities]);

  const connectOisy = useCallback(async () => {
    setOisyConnecting(true);
    setError(null);
    try {
      await connectToOisy(true);
    } finally {
      setOisyConnecting(false);
    }
  }, [connectToOisy]);

  const connectMetaMask = useCallback(async () => {
    const provider = getMetaMaskProvider();
    if (provider === null) {
      setError("wallet.metamask_missing");
      return;
    }
    setMetaMaskConnecting(true);
    setError(null);
    try {
      const chainIdHex = await ensureMetaMaskChain(provider, metaMaskChain);
      const accounts = await provider.request({ method: "eth_requestAccounts" });
      if (!Array.isArray(accounts) || !accounts.every((value) => typeof value === "string")) {
        throw new Error("wallet.metamask_accounts_invalid");
      }
      const firstAccount = accounts[0];
      if (firstAccount === undefined) {
        throw new Error("wallet.metamask_account_missing");
      }
      setMetaMaskSession({
        accountAddress: normalizeMetaMaskAddress(firstAccount),
        chainIdHex,
      });
    } catch (nextError) {
      setError(unknownToErrorMessage(nextError, "wallet.metamask_connect_failed"));
    } finally {
      setMetaMaskConnecting(false);
    }
  }, [metaMaskChain]);

  const disconnectOisy = useCallback(async () => {
    setError(null);
    await resetOisyConnectionState();
  }, [resetOisyConnectionState]);

  const disconnectMetaMask = useCallback(() => {
    setMetaMaskSession(null);
    setError(null);
  }, []);

  const clearError = useCallback(() => {
    setError(null);
  }, []);

  const getCaller = useCallback(async (): Promise<AuthenticatedCaller | null> => {
    return callerRef.current;
  }, []);

  const value = useMemo<WalletContextValue>(() => ({
    oisySession,
    metaMaskSession,
    oisyConnecting,
    metaMaskConnecting,
    metaMaskAvailable,
    oisyCapabilities,
    error,
    connectOisy,
    connectMetaMask,
    disconnectOisy,
    disconnectMetaMask,
    clearError,
    getCaller,
  }), [
    oisySession,
    metaMaskSession,
    oisyConnecting,
    metaMaskConnecting,
    metaMaskAvailable,
    oisyCapabilities,
    error,
    connectOisy,
    connectMetaMask,
    disconnectOisy,
    disconnectMetaMask,
    clearError,
    getCaller,
  ]);

  return <WalletContext.Provider value={value}>{children}</WalletContext.Provider>;
}

export const walletProviderTestHooks = {
  mapPrincipalToSession,
  resolveOisyPrincipalText,
};
