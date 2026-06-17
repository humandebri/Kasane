# Indexer Spec v1

Status: implemented and operated against the current canister export APIs.

Primary APIs:

- `export_blocks(cursor, max_bytes)`
- `rpc_eth_get_logs_paged(filter, cursor, limit)`

Hash policy:

- `tx_id` is the internal canister identifier.
- `eth_tx_hash` is `keccak256(raw_tx)` for Ethereum-compatible lookup.

Pruning is independent of external acknowledgements. The canister may prune according to retention and capacity policy even if the external indexer is offline.

## Recommended Architecture

1. Run a long-lived external worker.
2. Poll `export_blocks(cursor, max_bytes)`.
3. Decode returned chunks.
4. Insert or upsert derived rows into Postgres.
5. Persist cursor in Postgres after the write transaction commits.

Treat the external database as a cache. Chain correctness and block production must not depend on the indexer.

Recommended `max_bytes`: `1_000_000` to `1_500_000`.

## Storage Model

Use plain Postgres partitioning from the start for high-growth tables.

Recommended base schema:

```sql
create table if not exists blocks (
  number bigint primary key,
  hash bytea not null,
  parent_hash bytea not null,
  ts bigint not null,
  tx_count int not null
);

create table if not exists transactions (
  hash bytea primary key,
  block_number bigint not null,
  tx_index int not null,
  "from" bytea not null,
  "to" bytea,
  nonce bigint not null,
  value numeric(78,0) not null,
  gas bigint not null
) partition by range (block_number);

create table if not exists receipts (
  tx_hash bytea primary key,
  block_number bigint not null,
  status smallint not null,
  gas_used bigint not null,
  contract_address bytea
) partition by range (block_number);

create table if not exists logs (
  block_number bigint not null,
  tx_hash bytea not null,
  log_index int not null,
  address bytea not null,
  topic0 bytea,
  topic1 bytea,
  topic2 bytea,
  topic3 bytea,
  data bytea not null,
  primary key (block_number, tx_hash, log_index)
) partition by range (block_number);

create index if not exists logs_addr_topic_block
  on logs (address, topic0, block_number desc);
create index if not exists txs_block
  on transactions (block_number, tx_index);
create index if not exists receipts_block
  on receipts (block_number);
```

Use `bytea` for hashes, topics, and addresses. Avoid hex strings in indexed storage.

## Worker Loop

- `get_head()` reads current head.
- `export_blocks(cursor, max_bytes)` is called repeatedly.
- Writes are idempotent through `INSERT ... ON CONFLICT`.
- Cursor is advanced only after DB commit.
- When caught up, poll at a fixed interval.
- Use exponential backoff only for network failures.
- Retry failures at the same cursor.
- On `Err.Pruned`, rebase to `pruned_before_block + 1`, clamp to `1..head`, persist the cursor, and continue syncing.
- Stop and alert on repeated decode errors or invalid cursor errors.

## Export API

```text
get_head() -> u64
export_blocks(cursor, max_bytes) -> { chunks, next_cursor }
```

`cursor = null` starts from `oldest_exportable_block`, normally `pruned_before_block + 1`.

Each logical block bundle has four payloads:

- `block`
- `receipts`
- `tx_index`
- `internal_traces`

The export API returns `Chunk` slices, not length-prefixed bundles.

## Payload Encoding

Common rules:

- `tx_id` is exactly 32 raw bytes.
- `tx_id` is not `eth_tx_hash`.
- Length fields are `u32be`.
- `len = 0` is allowed.
- `payload_len` must fit `u32`.
- Breaking changes require a new API name.

Block payload:

```text
block_bytes
```

Receipts payload:

```text
repeat { tx_id(32) + u32be(len) + bytes }
```

Transaction index payload:

```text
repeat { tx_id(32) + u32be(len) + bytes }
```

The entry body includes block number, transaction index, caller principal, sender, recipient, selector, and optional `eth_tx_hash`.

Internal traces payload:

```text
repeat { tx_id(32) + u32be(len) + bytes }
```

The entry body includes a version byte.

## Cursor

Use numeric tags rather than an enum for wire compatibility.

```candid
type Cursor = record {
  block_number: nat64;
  segment: nat8;
  byte_offset: nat32;
};
```

Segment values:

- `0`: block
- `1`: receipts
- `2`: tx_index
- `3`: internal_traces

`byte_offset` is an offset inside the payload and excludes any external prefix.

## Chunk Rules

```candid
type Chunk = record {
  segment: nat8;
  start: nat32;
  bytes: blob;
  payload_len: nat32;
};
```

- `chunks[0].segment` must match `cursor.segment`.
- `chunks[0].start` must match `cursor.byte_offset`.
- Chunks are monotonic within one block by `segment` then `start`.
- `next_cursor` points to the exclusive end of the returned bytes.
- Within a segment, `next.start == prev.start + prev.bytes.len`.
- On segment change, `prev.start + prev.bytes.len == prev.payload_len`.
- One response covers at most one `block_number`.
- Segment order is `block -> receipts -> tx_index -> internal_traces`.

Cursor advancement:

- Move within a payload by increasing `byte_offset`.
- At payload end, increment `segment` and reset `byte_offset` to `0`.
- After segment `3`, increment `block_number`, set `segment=0`, and set `byte_offset=0`.

## Validation

- `segment > 3` returns `InvalidCursor`.
- `byte_offset > payload_len` returns `InvalidCursor`.
- `byte_offset == payload_len` is valid.
- `start + bytes.len <= payload_len` must hold.
- `payload_len <= max_segment_len` must hold.
- `sum(chunks.bytes.len) <= max_bytes` must hold.

When caught up, return `chunks=[]` and `next_cursor=cursor`.

If `cursor.block_number <= pruned_before_block`, return `Pruned { pruned_before_block }`.

## Cursor JSON

Workers persist cursor as JSON:

```json
{
  "v": 1,
  "block_number": "u64",
  "segment": 0,
  "byte_offset": 0
}
```

- `block_number` is decimal ASCII without leading zeroes, except `"0"`.
- `segment` is `0`, `1`, `2`, or `3`.
- `byte_offset` is `0..=u32`.

This avoids silent precision loss in JavaScript when block numbers exceed `2^53 - 1`.

## Metrics and Alerts

Recommended metrics:

- `export_lag_blocks = head - cursor`
- `export_lag_seconds`
- `last_export_at`
- `export_rate_blocks_per_min`
- `db_write_latency_ms`
- `db_batch_size`
- `errors_per_min`

Alert examples:

- lag above threshold for a sustained period
- repeated error increase
- DB write latency above threshold
- pruning approaching high-water or hard-emergency thresholds

## Minimal Worker v2 Contract

The current worker implementation uses a Postgres-first subset:

- `meta(key primary key, value)` for cursor, schema version, last head, last ingest time, and optional last error.
- `blocks(number primary key, hash, timestamp, tx_count)`.
- `txs(tx_hash primary key, block_number, tx_index)`.
- `metrics_daily(day primary key, raw_bytes, compressed_bytes, archive_bytes, blocks_ingested, errors)`.

Optional archive storage should be introduced before enabling aggressive automatic pruning. Its purpose is investigation and rebuild support after canister-side pruning.
