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
  const ExportResult = IDL.Variant({ Ok: ExportResponse, Err: ExportError });
  return IDL.Service({
    export_blocks: IDL.Func([IDL.Opt(Cursor), IDL.Nat32], [ExportResult], ["query"]),
    rpc_eth_block_number: IDL.Func([], [IDL.Nat64], ["query"]),
  });
};
