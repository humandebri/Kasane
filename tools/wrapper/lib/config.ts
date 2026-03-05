// どこで: wrapper設定解決 / 何を: 環境変数を型安全に読み込む / なぜ: client直呼び構成でも設定不足を早期検知するため

type WrapperConfig = {
  icHost: string;
  evmGatewayCanisterId: string;
  wrapCanisterId: string;
  fetchRootKey: boolean;
};

const REQUIRED_KEYS = ["NEXT_PUBLIC_IC_HOST", "EVM_GATEWAY_CANISTER_ID", "WRAP_CANISTER_ID"] as const;

function requiredEnv(name: (typeof REQUIRED_KEYS)[number], env: NodeJS.ProcessEnv): string {
  const value = env[name];
  if (value === undefined || value.trim() === "") {
    throw new Error(`config.missing:${name}`);
  }
  return value.trim();
}

function parseBoolean(value: string | undefined, fallback: boolean): boolean {
  if (value === undefined || value.trim() === "") {
    return fallback;
  }
  const normalized = value.trim().toLowerCase();
  if (normalized === "1" || normalized === "true" || normalized === "yes") {
    return true;
  }
  if (normalized === "0" || normalized === "false" || normalized === "no") {
    return false;
  }
  return fallback;
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): WrapperConfig {
  return {
    icHost: requiredEnv("NEXT_PUBLIC_IC_HOST", env),
    evmGatewayCanisterId: requiredEnv("EVM_GATEWAY_CANISTER_ID", env),
    wrapCanisterId: requiredEnv("WRAP_CANISTER_ID", env),
    fetchRootKey: parseBoolean(env.FETCH_ROOT_KEY, false),
  };
}
