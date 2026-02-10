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
  const ExportResult = IDL.Variant({ Ok: ExportResponse, Err: ExportError });
  return IDL.Service({
    export_blocks: IDL.Func([IDL.Opt(Cursor), IDL.Nat32], [ExportResult], ["query"]),
    get_prune_status: IDL.Func([], [PruneStatusView], ["query"]),
    rpc_eth_block_number: IDL.Func([], [IDL.Nat64], ["query"]),
  });
};
