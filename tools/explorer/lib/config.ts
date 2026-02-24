// どこで: Explorer設定層 / 何を: 実行時パラメータを集約 / なぜ: 依存値の散在を防ぎ安全に変更するため
import { MAX_CHAIN_ID_INT4 } from "./verify/constants";

export type ExplorerConfig = {
  databaseUrl: string;
  dbPoolMax: number;
  canisterId: string | null;
  icHost: string;
  rpcGatewayUrl: string | null;
  fetchRootKey: boolean;
  latestBlocksLimit: number;
  latestTxsLimit: number;
  principalTxsLimit: number;
  verifyEnabled: boolean;
  verifyRawPayloadLimitBytes: number;
  verifyDefaultChainId: number;
  verifyWorkerConcurrency: number;
  verifyJobTimeoutMs: number;
  verifyMaxRetries: number;
  verifyHourlyLimit: number;
  verifyDailyLimit: number;
  verifyAllowedCompilerVersions: string[];
  verifyAuthHmacKeys: ReadonlyMap<string, string>;
  verifyRequiredScope: string;
  verifyAdminUsers: Set<string>;
  verifySourcifyEnabled: boolean;
  verifySourcifyBaseUrl: string;
  verifyLogRetentionDays: number;
  verifyMetricsRetentionDays: number;
  verifyMetricsSampleIntervalMs: number;
  verifyAuditHashSaltCurrent: string;
  verifyAuditHashSaltPrevious: string | null;
};

const DEFAULT_IC_HOST = "https://icp-api.io";
const DEFAULT_DB_POOL_MAX = 10;

export function loadConfig(env: NodeJS.ProcessEnv): ExplorerConfig {
  const databaseUrl = env.EXPLORER_DATABASE_URL ?? env.INDEXER_DATABASE_URL;
  if (!databaseUrl) {
    throw new Error("EXPLORER_DATABASE_URL is required");
  }
  return {
    databaseUrl,
    dbPoolMax: parseRangeInt(env.EXPLORER_DB_POOL_MAX ?? env.INDEXER_DB_POOL_MAX, DEFAULT_DB_POOL_MAX, 1, 50),
    canisterId: env.EVM_CANISTER_ID ?? null,
    icHost: env.EXPLORER_IC_HOST ?? env.INDEXER_IC_HOST ?? DEFAULT_IC_HOST,
    rpcGatewayUrl: parseOptionalNonEmpty(env.EXPLORER_RPC_GATEWAY_URL ?? env.RPC_GATEWAY_URL),
    fetchRootKey: parseBool(env.EXPLORER_FETCH_ROOT_KEY ?? env.INDEXER_FETCH_ROOT_KEY),
    latestBlocksLimit: parseRangeInt(env.EXPLORER_LATEST_BLOCKS, 10, 1, 500),
    latestTxsLimit: parseRangeInt(env.EXPLORER_LATEST_TXS, 20, 1, 200),
    principalTxsLimit: parseRangeInt(env.EXPLORER_PRINCIPAL_TXS, 50, 1, 500),
    verifyEnabled: parseBool(env.EXPLORER_VERIFY_ENABLED),
    verifyRawPayloadLimitBytes: parseRangeInt(env.EXPLORER_VERIFY_MAX_PAYLOAD_BYTES, 5_000_000, 1024, 20_000_000),
    verifyDefaultChainId: parseRangeInt(env.EXPLORER_VERIFY_DEFAULT_CHAIN_ID, 0, 0, MAX_CHAIN_ID_INT4),
    verifyWorkerConcurrency: parseRangeInt(env.EXPLORER_VERIFY_WORKER_CONCURRENCY, 2, 1, 8),
    verifyJobTimeoutMs: parseRangeInt(env.EXPLORER_VERIFY_JOB_TIMEOUT_MS, 120_000, 1_000, 600_000),
    verifyMaxRetries: parseRangeInt(env.EXPLORER_VERIFY_MAX_RETRIES, 2, 0, 10),
    verifyHourlyLimit: parseRangeInt(env.EXPLORER_VERIFY_HOURLY_LIMIT, 10, 1, 10_000),
    verifyDailyLimit: parseRangeInt(env.EXPLORER_VERIFY_DAILY_LIMIT, 100, 1, 100_000),
    verifyAllowedCompilerVersions: parseCsv(env.EXPLORER_VERIFY_ALLOWED_COMPILER_VERSIONS),
    verifyAuthHmacKeys: parseVerifyHmacKeys(env.EXPLORER_VERIFY_AUTH_HMAC_KEYS),
    verifyRequiredScope: env.EXPLORER_VERIFY_REQUIRED_SCOPE?.trim() || "verify.submit",
    verifyAdminUsers: new Set(parseCsv(env.EXPLORER_VERIFY_ADMIN_USERS)),
    verifySourcifyEnabled: parseBool(env.EXPLORER_VERIFY_SOURCIFY_ENABLED ?? "1"),
    verifySourcifyBaseUrl: env.EXPLORER_VERIFY_SOURCIFY_BASE_URL?.trim() || "https://repo.sourcify.dev",
    verifyLogRetentionDays: parseRangeInt(env.EXPLORER_VERIFY_LOG_RETENTION_DAYS, 30, 1, 365),
    verifyMetricsRetentionDays: parseRangeInt(env.EXPLORER_VERIFY_METRICS_RETENTION_DAYS, 30, 14, 365),
    verifyMetricsSampleIntervalMs: parseRangeInt(env.EXPLORER_VERIFY_METRICS_SAMPLE_INTERVAL_MS, 30_000, 5_000, 300_000),
    verifyAuditHashSaltCurrent: env.AUDIT_HASH_SALT_CURRENT?.trim() || "",
    verifyAuditHashSaltPrevious: env.AUDIT_HASH_SALT_PREVIOUS?.trim() || null,
  };
}

function parseOptionalNonEmpty(value: string | undefined): string | null {
  if (!value) {
    return null;
  }
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
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

function parseCsv(value: string | undefined): string[] {
  if (!value) {
    return [];
  }
  return value
    .split(",")
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
}

function parseVerifyHmacKeys(value: string | undefined): ReadonlyMap<string, string> {
  const out = new Map<string, string>();
  if (!value) {
    return out;
  }
  for (const chunk of value.split(",")) {
    const [kidRaw, secretRaw] = chunk.split(":", 2);
    const kid = kidRaw?.trim() ?? "";
    const secret = secretRaw?.trim() ?? "";
    if (!kid || !secret) {
      continue;
    }
    out.set(kid, secret);
  }
  return out;
}
