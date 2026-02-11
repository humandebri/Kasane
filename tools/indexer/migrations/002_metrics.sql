create table if not exists metrics_daily (
  day integer primary key,
  raw_bytes bigint not null default 0,
  compressed_bytes bigint not null default 0,
  archive_bytes bigint,
  blocks_ingested bigint not null default 0,
  errors bigint not null default 0
);
