// どこで: MetaMask 補助 / 何を: provider 検出と network 整合を最小実装する / なぜ: unwrap の EVM 送信を wallet 拡張へ委譲するため

import { bytesToHex } from "@/lib/utils";

export type EthereumProviderRequest = {
  method: string;
  params?: readonly unknown[] | Record<string, unknown>;
};

export type EthereumProvider = {
  isMetaMask?: boolean;
  request: (args: EthereumProviderRequest) => Promise<unknown>;
  on: (event: "accountsChanged" | "chainChanged", listener: (value: unknown) => void) => void;
  removeListener: (event: "accountsChanged" | "chainChanged", listener: (value: unknown) => void) => void;
};

declare global {
  interface Window {
    ethereum?: EthereumProvider;
  }
}

export type MetaMaskChainConfig = {
  chainId: bigint;
  chainName: string;
  rpcUrl: string;
  nativeCurrencySymbol: string;
  blockExplorerUrl: string | null;
};

type ProviderErrorLike = {
  code?: unknown;
  message?: unknown;
};

function errorField(error: unknown, field: keyof ProviderErrorLike): unknown {
  if (typeof error !== "object" || error === null) {
    return null;
  }
  return Reflect.get(error, field);
}

function ensureString(value: unknown, code: string): string {
  if (typeof value !== "string" || value.trim() === "") {
    throw new Error(code);
  }
  return value;
}

export function getMetaMaskProvider(): EthereumProvider | null {
  if (typeof window === "undefined") {
    return null;
  }
  return window.ethereum ?? null;
}

export function normalizeChainIdHex(chainId: bigint): string {
  return `0x${chainId.toString(16)}`;
}

export function buildWalletAddEthereumChainParams(config: MetaMaskChainConfig): {
  chainId: string;
  chainName: string;
  nativeCurrency: {
    name: string;
    symbol: string;
    decimals: number;
  };
  rpcUrls: string[];
  blockExplorerUrls?: string[];
} {
  const out = {
    chainId: normalizeChainIdHex(config.chainId),
    chainName: config.chainName,
    nativeCurrency: {
      name: config.nativeCurrencySymbol,
      symbol: config.nativeCurrencySymbol,
      decimals: 18,
    },
    rpcUrls: [config.rpcUrl],
  };
  if (config.blockExplorerUrl === null) {
    return out;
  }
  return {
    ...out,
    blockExplorerUrls: [config.blockExplorerUrl],
  };
}

export async function requestMetaMaskAccounts(provider: EthereumProvider): Promise<string[]> {
  const result = await provider.request({ method: "eth_requestAccounts" });
  if (!Array.isArray(result) || !result.every((value) => typeof value === "string")) {
    throw new Error("wallet.metamask_accounts_invalid");
  }
  return result;
}

export async function getMetaMaskAccounts(provider: EthereumProvider): Promise<string[]> {
  const result = await provider.request({ method: "eth_accounts" });
  if (!Array.isArray(result) || !result.every((value) => typeof value === "string")) {
    throw new Error("wallet.metamask_accounts_invalid");
  }
  return result;
}

export async function getMetaMaskChainId(provider: EthereumProvider): Promise<string> {
  return ensureString(
    await provider.request({ method: "eth_chainId" }),
    "wallet.metamask_chain_id_invalid",
  );
}

export async function ensureMetaMaskChain(
  provider: EthereumProvider,
  config: MetaMaskChainConfig,
): Promise<string> {
  const expectedChainId = normalizeChainIdHex(config.chainId);
  const currentChainId = await getMetaMaskChainId(provider);
  if (currentChainId.toLowerCase() === expectedChainId.toLowerCase()) {
    return currentChainId;
  }
  try {
    await provider.request({
      method: "wallet_switchEthereumChain",
      params: [{ chainId: expectedChainId }],
    });
  } catch (error) {
    if (!isUnknownChainError(error)) {
      throw error;
    }
    await provider.request({
      method: "wallet_addEthereumChain",
      params: [buildWalletAddEthereumChainParams(config)],
    });
  }
  return getMetaMaskChainId(provider);
}

export function errorCodeOf(error: unknown): number | null {
  const code = errorField(error, "code");
  return typeof code === "number" ? code : null;
}

export function isUnknownChainError(error: unknown): boolean {
  if (errorCodeOf(error) === 4902) {
    return true;
  }
  const rawMessage = error instanceof Error
    ? error.message
    : typeof errorField(error, "message") === "string"
      ? errorField(error, "message")
      : "";
  const message = typeof rawMessage === "string" ? rawMessage : "";
  return message.includes("4902");
}

export function normalizeMetaMaskAddress(address: string): string {
  const trimmed = address.trim().toLowerCase();
  if (!/^0x[0-9a-f]{40}$/u.test(trimmed)) {
    throw new Error("wallet.metamask_address_invalid");
  }
  return trimmed;
}

export function unknownToErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

export function parseMetaMaskAccountsChanged(value: unknown): string[] {
  if (!Array.isArray(value) || !value.every((item) => typeof item === "string")) {
    return [];
  }
  return value;
}

export function parseMetaMaskChainChanged(value: unknown): string | null {
  if (typeof value !== "string" || value.trim() === "") {
    return null;
  }
  return value;
}

export const metaMaskTestHooks = {
  normalizeChainIdHex,
  buildWalletAddEthereumChainParams,
  normalizeMetaMaskAddress,
  parseMetaMaskAccountsChanged,
  parseMetaMaskChainChanged,
  errorCodeOf,
  isUnknownChainError,
  toHexNonce: (value: bigint): string => bytesToHex(Uint8Array.of(Number(value & 0xffn))),
};
