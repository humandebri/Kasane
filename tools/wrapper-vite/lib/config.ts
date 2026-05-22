// どこで: wrapper設定解決 / 何を: 環境変数を型安全に読み込む / なぜ: client直呼び構成でも設定不足を早期検知するため

export type WrapperConfig = {
  icHost: string;
  icpTokenListUrl: string;
  kasaneEvmCanisterId: string;
  wrapCanisterId: string;
  evmWrapFactory: string;
  kasaneRpcUrl: string;
  kasaneChainId: bigint;
  kasaneChainName: string;
  kasaneNativeCurrencySymbol: string;
  kasaneBlockExplorerUrl: string | null;
};

export type EnvMap = Record<string, string | undefined>;

function optionalEnv(name: "VITE_KASANE_BLOCK_EXPLORER_URL", env: EnvMap): string | null {
  const value = env[name];
  if (value === undefined) {
    return null;
  }
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

const REQUIRED_KEYS = [
  "VITE_IC_HOST",
  "VITE_ICP_TOKEN_LIST_URL",
  "VITE_KASANE_EVM_CANISTER_ID",
  "VITE_EVM_WRAP_FACTORY",
  "VITE_KASANE_RPC_URL",
  "VITE_KASANE_CHAIN_ID",
  "VITE_KASANE_CHAIN_NAME",
  "VITE_KASANE_NATIVE_CURRENCY_SYMBOL",
] as const;

function getBundledEnv(): EnvMap {
  const bundledImportMetaEnv = import.meta.env;
  return {
    VITE_IC_HOST: bundledImportMetaEnv?.VITE_IC_HOST,
    VITE_ICP_TOKEN_LIST_URL: bundledImportMetaEnv?.VITE_ICP_TOKEN_LIST_URL,
    VITE_KASANE_EVM_CANISTER_ID: bundledImportMetaEnv?.VITE_KASANE_EVM_CANISTER_ID,
    VITE_WRAP_CANISTER_ID: bundledImportMetaEnv?.VITE_WRAP_CANISTER_ID,
    VITE_EVM_WRAP_FACTORY: bundledImportMetaEnv?.VITE_EVM_WRAP_FACTORY,
    VITE_KASANE_RPC_URL: bundledImportMetaEnv?.VITE_KASANE_RPC_URL,
    VITE_KASANE_CHAIN_ID: bundledImportMetaEnv?.VITE_KASANE_CHAIN_ID,
    VITE_KASANE_CHAIN_NAME: bundledImportMetaEnv?.VITE_KASANE_CHAIN_NAME,
    VITE_KASANE_NATIVE_CURRENCY_SYMBOL: bundledImportMetaEnv?.VITE_KASANE_NATIVE_CURRENCY_SYMBOL,
    VITE_KASANE_BLOCK_EXPLORER_URL: bundledImportMetaEnv?.VITE_KASANE_BLOCK_EXPLORER_URL,
  };
}

function requiredEnv(name: (typeof REQUIRED_KEYS)[number], env: EnvMap): string {
  const value = env[name];
  if (value === undefined || value.trim() === "") {
    throw new Error(`config.missing:${name}`);
  }
  return value.trim();
}

function shouldFetchRootKey(icHost: string): boolean {
  return icHost.startsWith("http://127.0.0.1:")
    || icHost.startsWith("http://localhost:")
    || icHost.startsWith("https://127.0.0.1:")
    || icHost.startsWith("https://localhost:");
}

function parseChainId(value: string): bigint {
  const trimmed = value.trim();
  if (!/^[0-9]+$/u.test(trimmed)) {
    throw new Error("config.invalid:VITE_KASANE_CHAIN_ID");
  }
  return BigInt(trimmed);
}

export function loadConfigFromEnv(env: EnvMap): WrapperConfig {
  const icHost = requiredEnv("VITE_IC_HOST", env);
  const icpTokenListUrl = requiredEnv("VITE_ICP_TOKEN_LIST_URL", env);
  const kasaneEvmCanisterId = requiredEnv("VITE_KASANE_EVM_CANISTER_ID", env);
  return {
    icHost,
    icpTokenListUrl,
    kasaneEvmCanisterId,
    wrapCanisterId: kasaneEvmCanisterId,
    evmWrapFactory: requiredEnv("VITE_EVM_WRAP_FACTORY", env),
    kasaneRpcUrl: requiredEnv("VITE_KASANE_RPC_URL", env),
    kasaneChainId: parseChainId(requiredEnv("VITE_KASANE_CHAIN_ID", env)),
    kasaneChainName: requiredEnv("VITE_KASANE_CHAIN_NAME", env),
    kasaneNativeCurrencySymbol: requiredEnv("VITE_KASANE_NATIVE_CURRENCY_SYMBOL", env),
    kasaneBlockExplorerUrl: optionalEnv("VITE_KASANE_BLOCK_EXPLORER_URL", env),
  };
}

export function loadConfig(env: EnvMap = getBundledEnv()): WrapperConfig {
  return loadConfigFromEnv(env);
}

export const configTestHooks = {
  optionalEnv,
  loadConfigFromEnv,
  shouldFetchRootKey,
  parseChainId,
};
