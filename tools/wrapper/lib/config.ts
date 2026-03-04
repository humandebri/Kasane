// どこで: wrapper設定解決 / 何を: 環境変数を型安全に読み込む / なぜ: 未設定を起動時に即検知し運用事故を防ぐため

type WrapperConfig = {
  icHost: string;
  evmGatewayCanisterId: string;
  wrapCanisterId: string;
  fetchRootKey: boolean;
  submitIdentitySecretKeyHex: string | null;
};

const REQUIRED_KEYS = ["NEXT_PUBLIC_IC_HOST", "EVM_GATEWAY_CANISTER_ID"] as const;

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
  const icHost = requiredEnv("NEXT_PUBLIC_IC_HOST", env);
  const evmGatewayCanisterId = requiredEnv("EVM_GATEWAY_CANISTER_ID", env);
  const wrapCanisterId = (env.WRAP_CANISTER_ID ?? "").trim();
  if (wrapCanisterId === "") {
    throw new Error("config.missing:WRAP_CANISTER_ID");
  }
  const submitIdentitySecretKeyHex = (env.ICP_IDENTITY_SECRET_KEY_HEX ?? "").trim();
  return {
    icHost,
    evmGatewayCanisterId,
    wrapCanisterId,
    fetchRootKey: parseBoolean(env.FETCH_ROOT_KEY, false),
    submitIdentitySecretKeyHex:
      submitIdentitySecretKeyHex === "" ? null : submitIdentitySecretKeyHex,
  };
}
