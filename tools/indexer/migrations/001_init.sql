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
  caller_principal bytea,
  from_address bytea not null,
  to_address bytea,
  receipt_status smallint
);

create index if not exists idx_txs_block_number_tx_index_desc on txs(block_number desc, tx_index desc);
create index if not exists idx_txs_from_address_block_tx_desc on txs(from_address, block_number desc, tx_index desc);
create index if not exists idx_txs_to_address_block_tx_desc on txs(to_address, block_number desc, tx_index desc);
create index if not exists idx_txs_receipt_status_block_tx_desc on txs(receipt_status, block_number desc, tx_index desc);

create table if not exists token_transfers (
  tx_hash bytea not null references txs(tx_hash) on delete cascade,
  block_number bigint not null,
  tx_index integer not null,
  log_index integer not null,
  token_address bytea not null,
  from_address bytea not null,
  to_address bytea not null,
  amount_numeric numeric(78, 0) not null,
  primary key (tx_hash, log_index)
);

create index if not exists idx_token_transfers_from_block_tx_log_desc on token_transfers(from_address, block_number desc, tx_index desc, log_index desc);
create index if not exists idx_token_transfers_to_block_tx_log_desc on token_transfers(to_address, block_number desc, tx_index desc, log_index desc);
create index if not exists idx_token_transfers_token_block_tx_log_desc on token_transfers(token_address, block_number desc, tx_index desc, log_index desc);

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

create table if not exists ops_metrics_samples (
  sampled_at_ms bigint primary key,
  queue_len bigint not null,
  total_submitted bigint not null,
  total_included bigint not null,
  total_dropped bigint not null,
  drop_counts_json text not null
);

create index if not exists idx_ops_metrics_samples_sampled_at_desc on ops_metrics_samples(sampled_at_ms desc);

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
