create table if not exists schema_migrations (
  id text primary key,
  applied_at bigint not null
);

create table if not exists meta (
  key text primary key,
  value text
);

create table if not exists blocks (
  number bigint primary key,
  hash bytea,
  timestamp bigint not null,
  tx_count integer not null
);

create table if not exists txs (
  tx_hash bytea primary key,
  block_number bigint not null,
  tx_index integer not null,
  caller_principal bytea
);

create index if not exists idx_txs_block_number_tx_index_desc on txs(block_number desc, tx_index desc);

create table if not exists metrics_daily (
  day integer primary key,
  raw_bytes bigint not null default 0,
  compressed_bytes bigint not null default 0,
  archive_bytes bigint,
  blocks_ingested bigint not null default 0,
  errors bigint not null default 0
);

create table if not exists archive_parts (
  block_number bigint primary key,
  path text not null,
  sha256 bytea not null,
  size_bytes bigint not null,
  raw_bytes bigint not null,
  created_at bigint not null
);

create index if not exists idx_archive_parts_created_at_desc on archive_parts(created_at desc);

create table if not exists retention_runs (
  id text primary key,
  started_at bigint not null,
  finished_at bigint not null,
  retention_days integer not null,
  dry_run boolean not null,
  deleted_blocks bigint not null,
  deleted_txs bigint not null,
  deleted_metrics_daily bigint not null,
  deleted_archive_parts bigint not null,
  status text not null,
  error_message text
);
