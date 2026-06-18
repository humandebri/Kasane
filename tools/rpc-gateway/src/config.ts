// where: gateway config layer / what: centralizes runtime parameters / why: prevent misconfiguration of limits and upstream endpoints

declare const process: {
  env: Record<string, string | undefined>;
};

export type GatewayConfig = {
  canisterId: string;
  icHost: string;
  fetchRootKey: boolean;
  identityPem: string | null;
  host: string;
  port: number;
  clientVersion: string;
  rpcSemanticsVersion: string;
  maxHttpBodySize: number;
  maxBatchLen: number;
  maxJsonDepth: number;
  logsBlockhashScanLimit: number;
  corsOrigins: string[];
  x402Network: string;
  x402RpcUrl: string;
  x402SettlerPrivateKey: string | null;
};

type LoadConfigOptions = {
  requireCanisterId?: boolean;
};

export function loadConfig(env: Record<string, string | undefined>, options: LoadConfigOptions = {}): GatewayConfig {
  const rpcSemanticsVersion = parseOptionalNonEmpty(env.RPC_SEMANTICS_VERSION) ?? "kasane-rpc-semantics/v1";
  const baseClientVersion = env.RPC_GATEWAY_CLIENT_VERSION ?? "kasane/phase2-gateway/v0.1.0";
  const host = env.RPC_GATEWAY_HOST ?? "127.0.0.1";
  const port = parseRangeInt(env.RPC_GATEWAY_PORT, 8545, 1, 65535);
  return {
    canisterId: parseCanisterId(env.EVM_CANISTER_ID, options.requireCanisterId === true),
    icHost: env.RPC_GATEWAY_IC_HOST ?? "https://icp-api.io",
    fetchRootKey: parseBool(env.RPC_GATEWAY_FETCH_ROOT_KEY),
    identityPem: parseOptionalNonEmpty(env.RPC_GATEWAY_IDENTITY_PEM),
    host,
    port,
    clientVersion: `${baseClientVersion} ${rpcSemanticsVersion}`,
    rpcSemanticsVersion,
    maxHttpBodySize: parseRangeInt(env.RPC_GATEWAY_MAX_HTTP_BODY_SIZE, 256 * 1024, 1024, 10 * 1024 * 1024),
    maxBatchLen: parseRangeInt(env.RPC_GATEWAY_MAX_BATCH_LEN, 20, 1, 500),
    maxJsonDepth: parseRangeInt(env.RPC_GATEWAY_MAX_JSON_DEPTH, 20, 2, 100),
    logsBlockhashScanLimit: parseRangeInt(env.RPC_GATEWAY_LOGS_BLOCKHASH_SCAN_LIMIT, 2000, 100, 10000),
    corsOrigins: parseCorsOrigins(env.RPC_GATEWAY_CORS_ORIGIN),
    x402Network: parseOptionalNonEmpty(env.X402_NETWORK) ?? "eip155:4801360",
    x402RpcUrl: parseOptionalNonEmpty(env.X402_RPC_URL) ?? `http://${host}:${port}`,
    x402SettlerPrivateKey: parseOptionalNonEmpty(env.X402_SETTLER_PRIVATE_KEY),
  };
}

export function configureGateway(env: Record<string, string | undefined>, options: LoadConfigOptions = {}): void {
  const next = loadConfig(env, options);
  Object.assign(CONFIG, next);
}

function parseCanisterId(value: string | undefined, required: boolean): string {
  if (!value) {
    if (required) {
      throw new Error("EVM_CANISTER_ID is required");
    }
    return "";
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

function parseOptionalNonEmpty(value: string | undefined): string | null {
  if (!value) {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length === 0 ? null : trimmed;
}

function parseCorsOrigins(value: string | undefined): string[] {
  if (!value) {
    return ["*"];
  }
  const out = value
    .split(",")
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
  return out.length > 0 ? out : ["*"];
}

export const CONFIG = loadConfig(process.env);
