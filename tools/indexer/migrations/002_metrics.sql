create table if not exists metrics_daily (
  day integer primary key,
  raw_bytes integer,
  compressed_bytes integer,
  sqlite_growth_bytes integer,
  blocks_ingested integer,
  errors integer
);
