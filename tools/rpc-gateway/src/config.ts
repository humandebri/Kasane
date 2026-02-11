// どこで: Gateway設定層 / 何を: 実行時パラメータを一元管理 / なぜ: 制限値と接続先の誤設定を防ぐため

declare const process: {
  env: Record<string, string | undefined>;
};

export type GatewayConfig = {
  canisterId: string;
  icHost: string;
  fetchRootKey: boolean;
  host: string;
  port: number;
  clientVersion: string;
  maxHttpBodySize: number;
  maxBatchLen: number;
  maxJsonDepth: number;
  corsOrigin: string;
};

export function loadConfig(env: Record<string, string | undefined>): GatewayConfig {
  return {
    canisterId: required(env.EVM_CANISTER_ID, "EVM_CANISTER_ID is required"),
    icHost: env.RPC_GATEWAY_IC_HOST ?? "https://icp-api.io",
    fetchRootKey: parseBool(env.RPC_GATEWAY_FETCH_ROOT_KEY),
    host: env.RPC_GATEWAY_HOST ?? "127.0.0.1",
    port: parseRangeInt(env.RPC_GATEWAY_PORT, 8545, 1, 65535),
    clientVersion: env.RPC_GATEWAY_CLIENT_VERSION ?? "kasane/phase2-gateway/v0.1.0",
    maxHttpBodySize: parseRangeInt(env.RPC_GATEWAY_MAX_HTTP_BODY_SIZE, 256 * 1024, 1024, 10 * 1024 * 1024),
    maxBatchLen: parseRangeInt(env.RPC_GATEWAY_MAX_BATCH_LEN, 20, 1, 500),
    maxJsonDepth: parseRangeInt(env.RPC_GATEWAY_MAX_JSON_DEPTH, 20, 2, 100),
    corsOrigin: env.RPC_GATEWAY_CORS_ORIGIN ?? "*",
  };
}

function required(value: string | undefined, message: string): string {
  if (!value) {
    throw new Error(message);
  }
  return value;
}

function parseBool(value: string | undefined): boolean {
  if (!value) {
    return false;
  }
  return value === "1" || value.toLowerCase() === "true";
}

function parseRangeInt(value: string | undefined, fallback: number, min: number, max: number): number {
  if (!value) {
    return fallback;
  }
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed) || parsed < min || parsed > max) {
    return fallback;
  }
  return parsed;
}

export const CONFIG = loadConfig(process.env);
