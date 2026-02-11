// どこで: indexer補助スクリプト
// 何を: canister query経路を agent.query(Actor query) で検証
// なぜ: icp canister call が update 固定のため query検証を分離するため

import { Actor, HttpAgent } from "@dfinity/agent";
import { idlFactory } from "./candid";

type Cursor = {
  block_number: bigint;
  segment: number;
  byte_offset: number;
};

type Chunk = {
  segment: number;
  start: number;
  bytes: Uint8Array;
  payload_len: number;
};

type ExportResponse = {
  chunks: Chunk[];
  next_cursor: Cursor | null;
};

type ExportError =
  | { InvalidCursor: { message: string } }
  | { Pruned: { pruned_before_block: bigint } }
  | { MissingData: { message: string } }
  | { Limit: null };

type ExportResult = { Ok: ExportResponse } | { Err: ExportError };

type QueryActor = {
  rpc_eth_block_number: () => Promise<bigint>;
  export_blocks: (cursor: [] | [Cursor], maxBytes: number) => Promise<ExportResult>;
};

function envFlag(name: string, defaultValue: boolean): boolean {
  const raw = process.env[name];
  if (raw === undefined || raw.trim() === "") {
    return defaultValue;
  }
  return raw === "1" || raw.toLowerCase() === "true";
}

function envNat(name: string): bigint | null {
  const raw = process.env[name];
  if (raw === undefined || raw.trim() === "") {
    return null;
  }
  const value = BigInt(raw);
  if (value < 0n) {
    throw new Error(`${name} must be >= 0`);
  }
  return value;
}

function requireEnv(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`missing env: ${name}`);
  }
  return value;
}

async function main(): Promise<void> {
  const canisterId = requireEnv("EVM_CANISTER_ID");
  const host = process.env.INDEXER_IC_HOST ?? "http://127.0.0.1:4943";
  const fetchRootKey = process.env.INDEXER_FETCH_ROOT_KEY === "true";
  const maxBytes = Number(process.env.QUERY_SMOKE_MAX_BYTES ?? "65536");
  const requiredHeadMin = envNat("QUERY_SMOKE_REQUIRED_HEAD_MIN");
  const allowExportMissingData = envFlag("QUERY_SMOKE_ALLOW_EXPORT_MISSING_DATA", true);

  const agent = new HttpAgent({ host, fetch: globalThis.fetch });
  if (fetchRootKey) {
    await agent.fetchRootKey();
  }

  const actor = Actor.createActor<QueryActor>(idlFactory, { agent, canisterId });

  const head = await actor.rpc_eth_block_number();
  if (head < 0n) {
    throw new Error(`invalid head number: ${head}`);
  }
  if (requiredHeadMin !== null && head < requiredHeadMin) {
    throw new Error(`head ${head} is below required minimum ${requiredHeadMin}`);
  }

  const out = await actor.export_blocks([], maxBytes);
  if ("Err" in out) {
    if ("MissingData" in out.Err && !allowExportMissingData) {
      throw new Error(`export_blocks returned MissingData while strict mode is enabled: ${JSON.stringify(out.Err)}`);
    }
    if ("MissingData" in out.Err || "Pruned" in out.Err || "Limit" in out.Err) {
      console.log(`[query-smoke] export_blocks returned expected Err: ${JSON.stringify(out.Err)}`);
      console.log(`[query-smoke] ok head=${head} chunks=0 next_cursor=none`);
      return;
    }
    const keys = Object.keys(out.Err);
    throw new Error(`export_blocks returned unexpected Err (${keys.join(",")}): ${JSON.stringify(out.Err)}`);
  }

  const chunks = out.Ok.chunks.length;
  const next = out.Ok.next_cursor ? "some" : "none";
  console.log(`[query-smoke] ok head=${head} chunks=${chunks} next_cursor=${next}`);
}

main().catch((err) => {
  console.error(`[query-smoke] failed: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
});
