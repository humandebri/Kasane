// どこで: indexerの共有型 / 何を: export APIの型定義 / なぜ: 境界を明確にするため

export type Cursor = {
  block_number: bigint;
  segment: number;
  byte_offset: number;
};

export type Chunk = {
  segment: number;
  start: number;
  bytes: Uint8Array;
  payload_len: number;
};

export type ExportResponse = {
  chunks: Chunk[];
  next_cursor: Cursor | null;
};

export type CandidExportResponse = {
  chunks: Chunk[];
  next_cursor: [] | [Cursor];
};

export type ExportError =
  | { InvalidCursor: { message: string } }
  | { Pruned: { pruned_before_block: bigint } }
  | { MissingData: { message: string } }
  | { Limit: null };

export type Result<T, E> = { Ok: T } | { Err: E };

export type PruneStatusView = {
  pruning_enabled: boolean;
  prune_running: boolean;
  estimated_kept_bytes: bigint;
  high_water_bytes: bigint;
  low_water_bytes: bigint;
  hard_emergency_bytes: bigint;
  last_prune_at: bigint;
  pruned_before_block: bigint | null;
  oldest_kept_block: bigint | null;
  oldest_kept_timestamp: bigint | null;
  need_prune: boolean;
};

export type CandidOptNat64 = [] | [bigint] | bigint | null;

export type CandidPruneStatusView = {
  pruning_enabled: boolean;
  prune_running: boolean;
  estimated_kept_bytes: bigint;
  high_water_bytes: bigint;
  low_water_bytes: bigint;
  hard_emergency_bytes: bigint;
  last_prune_at: bigint;
  pruned_before_block: CandidOptNat64;
  oldest_kept_block: CandidOptNat64;
  oldest_kept_timestamp: CandidOptNat64;
  need_prune: boolean;
};

export type DropCountView = {
  code: number;
  count: bigint;
};

export type MetricsView = {
  txs: bigint;
  ema_txs_per_block_x1000: bigint;
  pruned_before_block: bigint | null;
  ema_block_rate_per_sec_x1000: bigint;
  total_submitted: bigint;
  window: bigint;
  avg_txs_per_block: bigint;
  block_rate_per_sec_x1000: bigint | null;
  cycles: bigint;
  total_dropped: bigint;
  blocks: bigint;
  drop_counts: DropCountView[];
  queue_len: bigint;
  total_included: bigint;
};

export type MemoryRegionView = {
  id: number;
  name: string;
  pages: bigint;
  bytes: bigint;
};

export type MemoryBreakdownView = {
  stable_pages_total: bigint;
  stable_bytes_total: bigint;
  regions_pages_total: bigint;
  regions_bytes_total: bigint;
  unattributed_stable_pages: bigint;
  unattributed_stable_bytes: bigint;
  heap_pages: bigint;
  heap_bytes: bigint;
  regions: MemoryRegionView[];
};

export type CandidMemoryRegionView = {
  id: number | string | bigint;
  name: string;
  pages: bigint | number | string;
  bytes: bigint | number | string;
};

export type CandidMemoryBreakdownView = {
  stable_pages_total: bigint | number | string;
  stable_bytes_total: bigint | number | string;
  regions_pages_total: bigint | number | string;
  regions_bytes_total: bigint | number | string;
  unattributed_stable_pages: bigint | number | string;
  unattributed_stable_bytes: bigint | number | string;
  heap_pages: bigint | number | string;
  heap_bytes: bigint | number | string;
  regions: CandidMemoryRegionView[];
};

export type CandidMetricsView = {
  txs: bigint;
  ema_txs_per_block_x1000: bigint;
  pruned_before_block: CandidOptNat64;
  ema_block_rate_per_sec_x1000: bigint;
  total_submitted: bigint;
  window: bigint;
  avg_txs_per_block: bigint;
  block_rate_per_sec_x1000: CandidOptNat64;
  cycles: bigint;
  total_dropped: bigint;
  blocks: bigint;
  drop_counts: DropCountView[];
  queue_len: bigint;
  total_included: bigint;
};

export type ExportActorMethods = {
  export_blocks: (cursor: [] | [Cursor], max_bytes: number) => Promise<Result<CandidExportResponse, ExportError>>;
  rpc_eth_get_transaction_by_tx_id: (txId: Uint8Array) => Promise<[] | [RpcTxView]>;
  rpc_eth_block_number: () => Promise<bigint>;
  get_prune_status: () => Promise<CandidPruneStatusView>;
  metrics: (window: bigint) => Promise<CandidMetricsView>;
  memory_breakdown: () => Promise<CandidMemoryBreakdownView>;
};

export type RpcTxDecodedView = {
  from: Uint8Array;
  to: [] | [Uint8Array];
  value: Uint8Array;
  nonce: bigint;
  gas_limit: bigint;
  input: Uint8Array;
  gas_price: [] | [bigint];
  max_fee_per_gas: [] | [bigint];
  max_priority_fee_per_gas: [] | [bigint];
  chain_id: [] | [bigint];
};

export type RpcTxView = {
  decoded: [] | [RpcTxDecodedView];
};
