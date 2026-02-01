// どこで: indexer設定 / 何を: 環境変数の読み込み / なぜ: 実行環境差分を吸収するため

export type Config = {
  canisterId: string;
  icHost: string;
  dbPath: string;
  maxBytes: number;
  backoffInitialMs: number;
  backoffMaxMs: number;
  fetchRootKey: boolean;
};

const DEFAULT_IC_HOST = "http://127.0.0.1:4943";
const DEFAULT_DB_PATH = "./indexer.sqlite";
const DEFAULT_MAX_BYTES = 1_200_000;
const DEFAULT_BACKOFF_INITIAL_MS = 200;
const DEFAULT_BACKOFF_MAX_MS = 5_000;

export function loadConfig(env: NodeJS.ProcessEnv): Config {
  const canisterId = env.INDEXER_CANISTER_ID;
  if (!canisterId) {
    throw new Error("INDEXER_CANISTER_ID is required");
  }
  const icHost = env.INDEXER_IC_HOST ?? DEFAULT_IC_HOST;
  const dbPath = env.INDEXER_DB_PATH ?? DEFAULT_DB_PATH;
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
  const fetchRootKey = env.INDEXER_FETCH_ROOT_KEY === "1" || env.INDEXER_FETCH_ROOT_KEY === "true";
  return {
    canisterId,
    icHost,
    dbPath,
    maxBytes,
    backoffInitialMs,
    backoffMaxMs,
    fetchRootKey,
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

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
