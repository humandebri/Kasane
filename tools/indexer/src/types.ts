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

export type ExportError =
  | { InvalidCursor: { message: string } }
  | { Pruned: { pruned_before_block: bigint } }
  | { MissingData: { message: string } }
  | { Limit: null };

export type Result<T, E> = { Ok: T } | { Err: E };

export type ExportActorMethods = {
  export_blocks: (cursor: [] | [Cursor], max_bytes: number) => Promise<Result<ExportResponse, ExportError>>;
  rpc_eth_block_number: () => Promise<bigint>;
};
