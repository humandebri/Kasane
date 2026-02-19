// どこで: Candidインターフェース定義 / 何を: export APIのIDL / なぜ: 型の一致を保証するため

import type { IDL } from "@dfinity/candid";

export const idlFactory: IDL.InterfaceFactory = ({ IDL }) => {
  const Cursor = IDL.Record({
    block_number: IDL.Nat64,
    segment: IDL.Nat8,
    byte_offset: IDL.Nat32,
  });
  const Chunk = IDL.Record({
    segment: IDL.Nat8,
    start: IDL.Nat32,
    bytes: IDL.Vec(IDL.Nat8),
    payload_len: IDL.Nat32,
  });
  const ExportResponse = IDL.Record({
    chunks: IDL.Vec(Chunk),
    next_cursor: IDL.Opt(Cursor),
  });
  const ExportError = IDL.Variant({
    InvalidCursor: IDL.Record({ message: IDL.Text }),
    Pruned: IDL.Record({ pruned_before_block: IDL.Nat64 }),
    MissingData: IDL.Record({ message: IDL.Text }),
    Limit: IDL.Null,
  });
  const PruneStatusView = IDL.Record({
    pruning_enabled: IDL.Bool,
    hard_emergency_bytes: IDL.Nat64,
    pruned_before_block: IDL.Opt(IDL.Nat64),
    low_water_bytes: IDL.Nat64,
    high_water_bytes: IDL.Nat64,
    oldest_kept_timestamp: IDL.Opt(IDL.Nat64),
    estimated_kept_bytes: IDL.Nat64,
    need_prune: IDL.Bool,
    last_prune_at: IDL.Nat64,
    prune_running: IDL.Bool,
    oldest_kept_block: IDL.Opt(IDL.Nat64),
  });
  const MemoryRegionView = IDL.Record({
    id: IDL.Nat8,
    name: IDL.Text,
    pages: IDL.Nat64,
    bytes: IDL.Nat64,
  });
  const MemoryBreakdownView = IDL.Record({
    stable_pages_total: IDL.Nat64,
    stable_bytes_total: IDL.Nat64,
    regions_pages_total: IDL.Nat64,
    regions_bytes_total: IDL.Nat64,
    unattributed_stable_pages: IDL.Nat64,
    unattributed_stable_bytes: IDL.Nat64,
    heap_pages: IDL.Nat64,
    heap_bytes: IDL.Nat64,
    regions: IDL.Vec(MemoryRegionView),
  });
  const DropCountView = IDL.Record({
    code: IDL.Nat16,
    count: IDL.Nat64,
  });
  const MetricsView = IDL.Record({
    txs: IDL.Nat64,
    ema_txs_per_block_x1000: IDL.Nat64,
    pruned_before_block: IDL.Opt(IDL.Nat64),
    ema_block_rate_per_sec_x1000: IDL.Nat64,
    total_submitted: IDL.Nat64,
    window: IDL.Nat64,
    avg_txs_per_block: IDL.Nat64,
    block_rate_per_sec_x1000: IDL.Opt(IDL.Nat64),
    cycles: IDL.Nat,
    total_dropped: IDL.Nat64,
    blocks: IDL.Nat64,
    drop_counts: IDL.Vec(DropCountView),
    queue_len: IDL.Nat64,
    total_included: IDL.Nat64,
  });
  const OpsModeView = IDL.Variant({ Low: IDL.Null, Normal: IDL.Null, Critical: IDL.Null });
  const OpsConfigView = IDL.Record({
    low_watermark: IDL.Nat,
    freeze_on_critical: IDL.Bool,
    critical: IDL.Nat,
  });
  const OpsStatusView = IDL.Record({
    needs_migration: IDL.Bool,
    critical_corrupt: IDL.Bool,
    decode_failure_last_ts: IDL.Nat64,
    log_filter_override: IDL.Opt(IDL.Text),
    last_cycle_balance: IDL.Nat,
    mode: OpsModeView,
    instruction_soft_limit: IDL.Nat64,
    last_check_ts: IDL.Nat64,
    mining_error_count: IDL.Nat64,
    log_truncated_count: IDL.Nat64,
    schema_version: IDL.Nat32,
    safe_stop_latched: IDL.Bool,
    decode_failure_last_label: IDL.Opt(IDL.Text),
    prune_error_count: IDL.Nat64,
    block_gas_limit: IDL.Nat64,
    config: OpsConfigView,
    decode_failure_count: IDL.Nat64,
  });
  const ExportResult = IDL.Variant({ Ok: ExportResponse, Err: ExportError });
  const RpcTxDecodedView = IDL.Record({
    input: IDL.Vec(IDL.Nat8),
  });
  const RpcTxView = IDL.Record({
    decoded: IDL.Opt(RpcTxDecodedView),
  });
  return IDL.Service({
    rpc_eth_chain_id: IDL.Func([], [IDL.Nat64], ["query"]),
    export_blocks: IDL.Func([IDL.Opt(Cursor), IDL.Nat32], [ExportResult], ["query"]),
    rpc_eth_get_transaction_by_tx_id: IDL.Func([IDL.Vec(IDL.Nat8)], [IDL.Opt(RpcTxView)], ["query"]),
    get_prune_status: IDL.Func([], [PruneStatusView], ["query"]),
    memory_breakdown: IDL.Func([], [MemoryBreakdownView], ["query"]),
    metrics: IDL.Func([IDL.Nat64], [MetricsView], ["query"]),
    rpc_eth_block_number: IDL.Func([], [IDL.Nat64], ["query"]),
    get_ops_status: IDL.Func([], [OpsStatusView], ["query"]),
  });
};
