// どこで: Explorer設定層 / 何を: 実行時パラメータを集約 / なぜ: 依存値の散在を防ぎ安全に変更するため

import path from "node:path";

export type ExplorerConfig = {
  dbPath: string;
  canisterId: string | null;
  icHost: string;
  fetchRootKey: boolean;
  latestBlocksLimit: number;
  latestTxsLimit: number;
};

const DEFAULT_DB_PATH = path.resolve(process.cwd(), "../indexer/indexer.sqlite");
const DEFAULT_IC_HOST = "https://icp-api.io";

export function loadConfig(env: NodeJS.ProcessEnv): ExplorerConfig {
  return {
    dbPath: env.EXPLORER_DB_PATH ?? env.INDEXER_DB_PATH ?? DEFAULT_DB_PATH,
    canisterId: env.EVM_CANISTER_ID ?? null,
    icHost: env.EXPLORER_IC_HOST ?? env.INDEXER_IC_HOST ?? DEFAULT_IC_HOST,
    fetchRootKey: parseBool(env.EXPLORER_FETCH_ROOT_KEY ?? env.INDEXER_FETCH_ROOT_KEY),
    latestBlocksLimit: parseRangeInt(env.EXPLORER_LATEST_BLOCKS, 10, 1, 100),
    latestTxsLimit: parseRangeInt(env.EXPLORER_LATEST_TXS, 20, 1, 200),
  };
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
