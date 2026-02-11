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
  tx_index integer not null
);

create index if not exists idx_txs_block_number_tx_index_desc on txs(block_number desc, tx_index desc);
