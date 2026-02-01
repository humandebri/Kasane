// どこで: canisterクライアント / 何を: export API呼び出し / なぜ: 取得処理を分離するため

import { Actor, HttpAgent } from "@dfinity/agent";
import { idlFactory } from "./candid";
import { Config } from "./config";
import { Cursor, ExportActorMethods, ExportError, ExportResponse, Result } from "./types";

export type ExportClient = {
  exportBlocks: (cursor: Cursor | null, maxBytes: number) => Promise<Result<ExportResponse, ExportError>>;
  getHeadNumber: () => Promise<bigint>;
};

export async function createClient(config: Config): Promise<ExportClient> {
  const fetchFn = globalThis.fetch;
  if (typeof fetchFn !== "function") {
    throw new Error("global fetch is not available; use Node 18+ or provide fetch");
  }
  const agent = new HttpAgent({ host: config.icHost, fetch: fetchFn });
  if (config.fetchRootKey) {
    await agent.fetchRootKey();
  }

  const actor = Actor.createActor<ExportActorMethods>(idlFactory, {
    agent,
    canisterId: config.canisterId,
  });

  return {
    exportBlocks: async (cursor: Cursor | null, maxBytes: number) => {
      const arg: [] | [Cursor] = cursor ? [cursor] : [];
      return actor.export_blocks(arg, maxBytes);
    },
    getHeadNumber: async () => actor.rpc_eth_block_number(),
  };
}
