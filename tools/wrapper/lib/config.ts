// どこで: wrapper設定解決 / 何を: 環境変数を型安全に読み込む / なぜ: client直呼び構成でも設定不足を早期検知するため

type WrapperConfig = {
  icHost: string;
  kasaneEvmCanisterId: string;
  wrapCanisterId: string;
  evmWrapFactory: string;
};

function optionalEnv(name: "NEXT_PUBLIC_INTERNET_IDENTITY_URL", env: NodeJS.ProcessEnv): string | null {
  const value = env[name];
  if (value === undefined) {
    return null;
  }
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

const REQUIRED_KEYS = ["NEXT_PUBLIC_IC_HOST", "KASANE_EVM_CANISTER_ID", "WRAP_CANISTER_ID", "EVM_WRAP_FACTORY"] as const;

type RequiredWrapperEnv = Pick<NodeJS.ProcessEnv, (typeof REQUIRED_KEYS)[number]>;

const BUNDLED_ENV: RequiredWrapperEnv = {
  NEXT_PUBLIC_IC_HOST: process.env.NEXT_PUBLIC_IC_HOST,
  KASANE_EVM_CANISTER_ID: process.env.KASANE_EVM_CANISTER_ID,
  WRAP_CANISTER_ID: process.env.WRAP_CANISTER_ID,
  EVM_WRAP_FACTORY: process.env.EVM_WRAP_FACTORY,
};

function requiredEnv(name: (typeof REQUIRED_KEYS)[number], env: RequiredWrapperEnv): string {
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

  const icHost = requiredEnv("NEXT_PUBLIC_IC_HOST", env);
export function loadConfig(env: RequiredWrapperEnv = BUNDLED_ENV): WrapperConfig {
  const icHost = requiredEnv("NEXT_PUBLIC_IC_HOST", env);
  return {
    icHost,
    kasaneEvmCanisterId: requiredEnv("KASANE_EVM_CANISTER_ID", env),
    wrapCanisterId: requiredEnv("WRAP_CANISTER_ID", env),
    evmWrapFactory: requiredEnv("EVM_WRAP_FACTORY", env),
  };
}

export function resolveConfiguredIdentityProvider(env: NodeJS.ProcessEnv = process.env): string | null {
  return optionalEnv("NEXT_PUBLIC_INTERNET_IDENTITY_URL", env);
}

export const configTestHooks = {
  optionalEnv,
  resolveConfiguredIdentityProvider,
  shouldFetchRootKey,
};
