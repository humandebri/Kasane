create table if not exists schema_migrations (
  id text primary key,
  applied_at integer not null
);

create table if not exists meta (
  key text primary key,
  value blob
);

create table if not exists blocks (
  number integer primary key,
  hash blob,
  timestamp integer not null,
  tx_count integer not null
);

create table if not exists txs (
  tx_hash blob primary key,
  block_number integer not null,
  tx_index integer not null
);
