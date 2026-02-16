// どこで: indexer設定 / 何を: 環境変数の読み込み / なぜ: 実行環境差分を吸収するため

import path from "path";

export type Config = {
  canisterId: string;
  icHost: string;
  databaseUrl: string;
  dbPoolMax: number;
  retentionDays: number;
  retentionEnabled: boolean;
  retentionDryRun: boolean;
  archiveGcDeleteOrphans: boolean;
  maxBytes: number;
  backoffInitialMs: number;
  backoffMaxMs: number;
  idlePollMs: number;
  pruneStatusPollMs: number;
  fetchRootKey: boolean;
  archiveDir: string;
  chainId: string;
  zstdLevel: number;
  maxSegment: number;
};

const DEFAULT_IC_HOST = "https://icp-api.io";
const DEFAULT_DB_POOL_MAX = 10;
const DEFAULT_RETENTION_DAYS = 90;
const DEFAULT_MAX_BYTES = 1_200_000;
const DEFAULT_BACKOFF_INITIAL_MS = 200;
const DEFAULT_BACKOFF_MAX_MS = 5_000;
const DEFAULT_IDLE_POLL_MS = 1_000;
const DEFAULT_ARCHIVE_DIR = "./archive";
const DEFAULT_CHAIN_ID = "4801360";
const DEFAULT_ZSTD_LEVEL = 3;
const DEFAULT_PRUNE_STATUS_POLL_MS = 30_000;
const DEFAULT_MAX_SEGMENT = 2;

export function loadConfig(env: NodeJS.ProcessEnv): Config {
  const canisterId = env.EVM_CANISTER_ID;
  if (!canisterId) {
    throw new Error("EVM_CANISTER_ID is required");
  }
  const icHost = env.INDEXER_IC_HOST ?? DEFAULT_IC_HOST;
  const databaseUrl = env.INDEXER_DATABASE_URL;
  if (!databaseUrl) {
    throw new Error("INDEXER_DATABASE_URL is required");
  }
  const dbPoolMax = readNumber(env.INDEXER_DB_POOL_MAX, DEFAULT_DB_POOL_MAX, "INDEXER_DB_POOL_MAX");
  const retentionDays = readNumber(env.INDEXER_RETENTION_DAYS, DEFAULT_RETENTION_DAYS, "INDEXER_RETENTION_DAYS");
  const retentionEnabled = parseBool(env.INDEXER_RETENTION_ENABLED, true);
  const retentionDryRun = parseBool(env.INDEXER_RETENTION_DRY_RUN, false);
  const archiveGcDeleteOrphans = parseBool(env.INDEXER_ARCHIVE_GC_DELETE_ORPHANS, false);
  const maxBytes = readNumber(env.INDEXER_MAX_BYTES, DEFAULT_MAX_BYTES, "INDEXER_MAX_BYTES");
  const backoffInitialMs = readNumber(
    env.INDEXER_BACKOFF_INITIAL_MS,
    DEFAULT_BACKOFF_INITIAL_MS,
    "INDEXER_BACKOFF_INITIAL_MS"
  );
  const backoffMaxMs = readNumber(
    env.INDEXER_BACKOFF_MAX_MS,
    DEFAULT_BACKOFF_MAX_MS,
    "INDEXER_BACKOFF_MAX_MS"
  );
  const idlePollMs = readNumber(env.INDEXER_IDLE_POLL_MS, DEFAULT_IDLE_POLL_MS, "INDEXER_IDLE_POLL_MS");
  const fetchRootKey = env.INDEXER_FETCH_ROOT_KEY === "1" || env.INDEXER_FETCH_ROOT_KEY === "true";
  const archiveDirRaw = env.INDEXER_ARCHIVE_DIR ?? DEFAULT_ARCHIVE_DIR;
  const archiveDir = path.resolve(archiveDirRaw);
  const chainId = env.INDEXER_CHAIN_ID ?? DEFAULT_CHAIN_ID;
  const zstdLevel = readNumber(env.INDEXER_ZSTD_LEVEL, DEFAULT_ZSTD_LEVEL, "INDEXER_ZSTD_LEVEL");
  const maxSegment = readNonNegativeNumber(env.INDEXER_MAX_SEGMENT, DEFAULT_MAX_SEGMENT, "INDEXER_MAX_SEGMENT");
  const pruneStatusPollMs = readNumber(
    env.INDEXER_PRUNE_STATUS_POLL_MS,
    DEFAULT_PRUNE_STATUS_POLL_MS,
    "INDEXER_PRUNE_STATUS_POLL_MS"
  );
  return {
    canisterId,
    icHost,
    databaseUrl,
    dbPoolMax,
    retentionDays,
    retentionEnabled,
    retentionDryRun,
    archiveGcDeleteOrphans,
    maxBytes,
    backoffInitialMs,
    backoffMaxMs,
    idlePollMs,
    pruneStatusPollMs,
    fetchRootKey,
    archiveDir,
    chainId,
    zstdLevel,
    maxSegment,
  };
}

function readNumber(value: string | undefined, fallback: number, name: string): number {
  if (!value) {
    return fallback;
  }
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive number`);
  }
  return Math.floor(parsed);
}

function parseBool(value: string | undefined, fallback: boolean): boolean {
  if (value === undefined) {
    return fallback;
  }
  return value === "1" || value.toLowerCase() === "true";
}

function readNonNegativeNumber(value: string | undefined, fallback: number, name: string): number {
  if (!value) {
    return fallback;
  }
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 0) {
    throw new Error(`${name} must be a non-negative number`);
  }
  return Math.floor(parsed);
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
