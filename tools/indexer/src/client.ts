// どこで: canisterクライアント / 何を: export API呼び出し / なぜ: 取得処理を分離するため

import { Actor, HttpAgent } from "@dfinity/agent";
import { idlFactory } from "./candid";
import { Config } from "./config";
import {
  CandidMemoryBreakdownView,
  CandidMetricsView,
  CandidMemoryRegionView,
  CandidOptNat64,
  CandidPruneStatusView,
  Cursor,
  ExportActorMethods,
  ExportError,
  ExportResponse,
  MemoryBreakdownView,
  MemoryRegionView,
  MetricsView,
  PruneStatusView,
  Result,
} from "./types";

export type ExportClient = {
  exportBlocks: (cursor: Cursor | null, maxBytes: number) => Promise<Result<ExportResponse, ExportError>>;
  getTxMetaByTxId: (txId: Uint8Array) => Promise<{ input: Uint8Array | null; ethTxHash: Uint8Array | null }>;
  getHeadNumber: () => Promise<bigint>;
  getPruneStatus: () => Promise<PruneStatusView>;
  getMetrics: (window: bigint) => Promise<MetricsView>;
  getMemoryBreakdown: () => Promise<MemoryBreakdownView>;
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
      const arg: [] | [Cursor] = cursor ? [normalizeCursorForCandid(cursor)] : [];
      const raw = await actor.export_blocks(arg, maxBytes);
      if ("Err" in raw) {
        return raw as Result<ExportResponse, ExportError>;
      }
      const nextCursor: Cursor | null =
        Array.isArray(raw.Ok.next_cursor) && raw.Ok.next_cursor.length === 1
          ? normalizeCursorForCandid(raw.Ok.next_cursor[0])
          : null;
      return {
        Ok: {
          chunks: raw.Ok.chunks,
          next_cursor: nextCursor,
        },
      };
    },
    getTxMetaByTxId: async (txId: Uint8Array) => {
      const out = await actor.rpc_eth_get_transaction_by_tx_id(txId);
      const first = out[0];
      if (!first) {
        return { input: null, ethTxHash: null };
      }
      const decoded = first.decoded[0];
      const ethTxHash = first.eth_tx_hash[0] ?? null;
      return { input: decoded ? decoded.input : null, ethTxHash };
    },
    getHeadNumber: async () => toNat64BigInt(await actor.rpc_eth_block_number(), "rpc_eth_block_number"),
    getPruneStatus: async () => normalizePruneStatus(await actor.get_prune_status()),
    getMetrics: async (window: bigint) => normalizeMetrics(await actor.metrics(window)),
    getMemoryBreakdown: async () => normalizeMemoryBreakdown(await actor.memory_breakdown()),
  };
}

type CursorInput = {
  block_number: bigint | number | string;
  segment: number | string;
  byte_offset: number | string;
};

function normalizeCursorForCandid(cursor: CursorInput): Cursor {
  return {
    block_number: toNat64BigInt(cursor.block_number, "cursor.block_number"),
    segment: toNat32Number(cursor.segment, "cursor.segment"),
    byte_offset: toNat32Number(cursor.byte_offset, "cursor.byte_offset"),
  };
}

function toNat64BigInt(value: bigint | number | string, name: string): bigint {
  if (typeof value === "bigint") {
    if (value < 0n) {
      throw new Error(`${name} must be non-negative`);
    }
    return value;
  }
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value) || value < 0) {
      throw new Error(`${name} must be a non-negative safe integer`);
    }
    return BigInt(value);
  }
  if (typeof value === "string") {
    if (!/^(0|[1-9][0-9]*)$/.test(value)) {
      throw new Error(`${name} must be a base-10 non-negative integer string`);
    }
    return BigInt(value);
  }
  throw new Error(`${name} must be bigint, number, or string`);
}

function toNat32Number(value: bigint | number | string, name: string): number {
  const parsed = typeof value === "bigint" ? Number(value) : typeof value === "number" ? value : Number(value);
  if (!Number.isInteger(parsed) || parsed < 0 || parsed > 0xffff_ffff) {
    throw new Error(`${name} must be an integer in 0..4294967295`);
  }
  return parsed;
}

function normalizePruneStatus(raw: CandidPruneStatusView): PruneStatusView {
  return {
    pruning_enabled: raw.pruning_enabled,
    prune_running: raw.prune_running,
    estimated_kept_bytes: toNat64BigInt(raw.estimated_kept_bytes, "prune_status.estimated_kept_bytes"),
    high_water_bytes: toNat64BigInt(raw.high_water_bytes, "prune_status.high_water_bytes"),
    low_water_bytes: toNat64BigInt(raw.low_water_bytes, "prune_status.low_water_bytes"),
    hard_emergency_bytes: toNat64BigInt(raw.hard_emergency_bytes, "prune_status.hard_emergency_bytes"),
    last_prune_at: toNat64BigInt(raw.last_prune_at, "prune_status.last_prune_at"),
    pruned_before_block: normalizeOptNat64(raw.pruned_before_block, "prune_status.pruned_before_block"),
    oldest_kept_block: normalizeOptNat64(raw.oldest_kept_block, "prune_status.oldest_kept_block"),
    oldest_kept_timestamp: normalizeOptNat64(raw.oldest_kept_timestamp, "prune_status.oldest_kept_timestamp"),
    need_prune: raw.need_prune,
  };
}

function normalizeMetrics(raw: CandidMetricsView): MetricsView {
  return {
    txs: toNat64BigInt(raw.txs, "metrics.txs"),
    ema_txs_per_block_x1000: toNat64BigInt(raw.ema_txs_per_block_x1000, "metrics.ema_txs_per_block_x1000"),
    pruned_before_block: normalizeOptNat64(raw.pruned_before_block, "metrics.pruned_before_block"),
    ema_block_rate_per_sec_x1000: toNat64BigInt(raw.ema_block_rate_per_sec_x1000, "metrics.ema_block_rate_per_sec_x1000"),
    total_submitted: toNat64BigInt(raw.total_submitted, "metrics.total_submitted"),
    window: toNat64BigInt(raw.window, "metrics.window"),
    avg_txs_per_block: toNat64BigInt(raw.avg_txs_per_block, "metrics.avg_txs_per_block"),
    block_rate_per_sec_x1000: normalizeOptNat64(raw.block_rate_per_sec_x1000, "metrics.block_rate_per_sec_x1000"),
    cycles: toNat64BigInt(raw.cycles, "metrics.cycles"),
    total_dropped: toNat64BigInt(raw.total_dropped, "metrics.total_dropped"),
    blocks: toNat64BigInt(raw.blocks, "metrics.blocks"),
    drop_counts: raw.drop_counts.map((item) => ({
      code: toNat32Number(item.code, "metrics.drop_counts.code"),
      count: toNat64BigInt(item.count, "metrics.drop_counts.count"),
    })),
    queue_len: toNat64BigInt(raw.queue_len, "metrics.queue_len"),
    total_included: toNat64BigInt(raw.total_included, "metrics.total_included"),
  };
}

function normalizeMemoryBreakdown(raw: CandidMemoryBreakdownView): MemoryBreakdownView {
  return {
    stable_pages_total: toNat64BigInt(raw.stable_pages_total, "memory_breakdown.stable_pages_total"),
    stable_bytes_total: toNat64BigInt(raw.stable_bytes_total, "memory_breakdown.stable_bytes_total"),
    regions_pages_total: toNat64BigInt(raw.regions_pages_total, "memory_breakdown.regions_pages_total"),
    regions_bytes_total: toNat64BigInt(raw.regions_bytes_total, "memory_breakdown.regions_bytes_total"),
    unattributed_stable_pages: toNat64BigInt(
      raw.unattributed_stable_pages,
      "memory_breakdown.unattributed_stable_pages"
    ),
    unattributed_stable_bytes: toNat64BigInt(
      raw.unattributed_stable_bytes,
      "memory_breakdown.unattributed_stable_bytes"
    ),
    heap_pages: toNat64BigInt(raw.heap_pages, "memory_breakdown.heap_pages"),
    heap_bytes: toNat64BigInt(raw.heap_bytes, "memory_breakdown.heap_bytes"),
    regions: raw.regions.map(normalizeMemoryRegion),
  };
}

function normalizeMemoryRegion(raw: CandidMemoryRegionView): MemoryRegionView {
  return {
    id: toNat32Number(raw.id, "memory_breakdown.region.id"),
    name: raw.name,
    pages: toNat64BigInt(raw.pages, "memory_breakdown.region.pages"),
    bytes: toNat64BigInt(raw.bytes, "memory_breakdown.region.bytes"),
  };
}

function normalizeOptNat64(value: CandidOptNat64, name: string): bigint | null {
  if (value === null) {
    return null;
  }
  if (Array.isArray(value)) {
    if (value.length === 0) {
      return null;
    }
    if (value.length === 1) {
      return toNat64BigInt(value[0], name);
    }
    throw new Error(`${name} opt must contain at most one value`);
  }
  return toNat64BigInt(value, name);
}

export const clientTestHooks = {
  normalizeCursorForCandid,
  normalizeMemoryBreakdown,
};
