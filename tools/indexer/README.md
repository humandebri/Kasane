# Indexer Worker

The indexer pulls canister export APIs and builds a minimal Postgres index.

## Usage

```bash
cd tools/indexer
npm install
cp .env.example .env.local
npm run dev
```

## Environment

`.env.example` is the distributable template. Create `.env.local` for local values.

- `EVM_CANISTER_ID` (required)
- `INDEXER_DATABASE_URL` (required, for example `postgres://postgres:postgres@127.0.0.1:5432/kasane`)
- `INDEXER_DB_POOL_MAX` (default: `10`)
- `INDEXER_RETENTION_ENABLED` (default: `true`)
- `INDEXER_RETENTION_DAYS` (default: `90`)
- `INDEXER_RETENTION_DRY_RUN` (default: `false`)
- `INDEXER_ARCHIVE_GC_DELETE_ORPHANS` (default: `false`)
- `INDEXER_IC_HOST` (default: `https://icp-api.io`)
- `INDEXER_MAX_BYTES` (default: `1200000`)
- `INDEXER_BACKOFF_INITIAL_MS` (default: `200`)
- `INDEXER_BACKOFF_MAX_MS` (default: `5000`)
- `INDEXER_IDLE_POLL_MS` (default: `1000`)
- `INDEXER_PRUNE_STATUS_POLL_MS` (default: `30000`)
- `INDEXER_OPS_METRICS_POLL_MS` (default: `30000`)
- `INDEXER_FETCH_ROOT_KEY` (`1`/`true` for local replica use)
- `INDEXER_CLIENT_REBUILD_RETRY_COUNT` (default: `6`)
- `INDEXER_ARCHIVE_DIR` (default: `./archive`)
- `INDEXER_CHAIN_ID` (default: `4801360`)
- `INDEXER_ZSTD_LEVEL` (default: `3`)
- `INDEXER_MAX_SEGMENT` (default: `3`)

For a local replica, set `INDEXER_IC_HOST=http://127.0.0.1:4943` and `INDEXER_FETCH_ROOT_KEY=true`.

## Cursor JSON

```json
{
  "v": 1,
  "block_number": "u64",
  "segment": 0,
  "byte_offset": 0
}
```

- `block_number` is decimal ASCII without leading zeroes, except `"0"`.
- `segment` must be `0..INDEXER_MAX_SEGMENT`.
- `byte_offset` must fit `u32`.
- If a stored cursor segment exceeds `INDEXER_MAX_SEGMENT`, startup stops.
- `Err.Pruned` from `export_blocks` is corrected to `pruned_before_block + 1`, clamped to `1..head`.

## Polling and Retry

- When caught up, poll at fixed `INDEXER_IDLE_POLL_MS`.
- Exponential backoff is used only for network failures.
- Cursor advances only after the Postgres write transaction commits.

## Indexed Data

- `archive_parts` is a rebuildable cache. Files not tied to DB rows may be removed during startup GC.
- `metrics_daily.archive_bytes` stores daily real size; deltas are calculated by readers.
- `txs.receipt_status` stores `0|1` extracted from receipt payloads in segment `1`.
- Internal transactions come from segment `3` internal trace payloads.
- ERC-20 transfers come from `Transfer(address,address,uint256)` logs in receipt payloads.
- Ops metrics are sampled from canister `metrics(128)` and retained for 14 days.

## Fixed Payload Layouts

Segment `2` transaction index entry:

```text
[tx_hash:32][entry_len:4][block_number:8][tx_index:4][caller_principal_len:2][caller_principal][from:20][to_len:1][to][selector_len:1][selector][eth_hash_len:1][eth_hash]
```

- All integers are Big Endian.
- `to_len` is `0` or `20`.
- `selector_len` is `0` or `4`.
- `eth_hash_len` is `0` or `32`.
- Older entries without `selector_len` are rejected.

Segment `3` internal trace payload:

```text
[tx_hash:32][entry_len:4][version:1][truncated:1][captured_count:4][total_count:4][trace_count:4][trace...]
```

Each trace:

```text
[block_number:8][tx_index:4][trace_id_len:2][trace_id][depth:2][action_type:1][from:20][to_len:1][to][value:32][created_len:1][created][success:1][error_len:2][error]
```

- `version=2` only.
- `captured_count` must equal `trace_count`.
- `to_len` and `created_len` are `0` or `20`.
- `value` is decoded as unsigned 256-bit integer and stored as `numeric(78,0)`.

## Backfill

```bash
cd tools/indexer
npm run backfill:eth-hash
```

Optional controls:

- `INDEXER_BACKFILL_BATCH_SIZE` (default: `200`)
- `INDEXER_BACKFILL_MAX_BATCHES` (default: `0`, unlimited)
- `INDEXER_BACKFILL_RETRY_MAX` (default: `2`)
- `INDEXER_BACKFILL_RETRY_SLEEP_MS` (default: `100`)

## Migrations

Startup reads `schema_migrations` and applies only missing SQL files from `tools/indexer/migrations/`.
