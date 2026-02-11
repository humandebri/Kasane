alter table if exists txs
  add column if not exists caller_principal bytea;

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

create or replace function run_retention_cleanup(
  p_retention_days integer default 90,
  p_dry_run boolean default false
)
returns table(
  run_id text,
  started_at_ms bigint,
  finished_at_ms bigint,
  retention_days integer,
  dry_run boolean,
  deleted_blocks bigint,
  deleted_txs bigint,
  deleted_metrics_daily bigint,
  deleted_archive_parts bigint,
  status text,
  error_message text
)
language sql
as $$
with params as (
  select
    (extract(epoch from now()) * 1000)::bigint as started_ms,
    extract(epoch from (now() - ((p_retention_days::text || ' days')::interval)))::bigint as cutoff_ts,
    (
      extract(year from (now() - ((p_retention_days::text || ' days')::interval)))::integer * 10000 +
      extract(month from (now() - ((p_retention_days::text || ' days')::interval)))::integer * 100 +
      extract(day from (now() - ((p_retention_days::text || ' days')::interval)))::integer
    ) as cutoff_day,
    ((extract(epoch from now()) * 1000)::bigint::text || '-' || p_retention_days::text) as id
),
would_delete_txs as (
  select count(*)::bigint as n
  from txs t
  join blocks b on b.number = t.block_number
  join params p on true
  where b.timestamp < p.cutoff_ts
),
would_delete_blocks as (
  select count(*)::bigint as n
  from blocks b
  join params p on true
  where b.timestamp < p.cutoff_ts
),
would_delete_metrics as (
  select count(*)::bigint as n
  from metrics_daily m
  join params p on true
  where m.day < p.cutoff_day
),
would_delete_archive as (
  select count(*)::bigint as n
  from archive_parts a
  join params p on true
  where a.created_at < p.cutoff_ts * 1000
),
deleted_txs as (
  delete from txs
  where not p_dry_run
    and block_number in (
      select number from blocks
      where timestamp < (select cutoff_ts from params)
    )
  returning 1
),
count_deleted_txs as (
  select case when p_dry_run then (select n from would_delete_txs) else count(*)::bigint end as n
  from deleted_txs
),
deleted_blocks as (
  delete from blocks
  where not p_dry_run
    and timestamp < (select cutoff_ts from params)
  returning 1
),
count_deleted_blocks as (
  select case when p_dry_run then (select n from would_delete_blocks) else count(*)::bigint end as n
  from deleted_blocks
),
deleted_metrics as (
  delete from metrics_daily
  where not p_dry_run
    and day < (select cutoff_day from params)
  returning 1
),
count_deleted_metrics as (
  select case when p_dry_run then (select n from would_delete_metrics) else count(*)::bigint end as n
  from deleted_metrics
),
deleted_archive as (
  delete from archive_parts
  where not p_dry_run
    and created_at < (select cutoff_ts from params) * 1000
  returning 1
),
count_deleted_archive as (
  select case when p_dry_run then (select n from would_delete_archive) else count(*)::bigint end as n
  from deleted_archive
),
ins as (
  insert into retention_runs(
    id,
    started_at,
    finished_at,
    retention_days,
    dry_run,
    deleted_blocks,
    deleted_txs,
    deleted_metrics_daily,
    deleted_archive_parts,
    status,
    error_message
  )
  select
    p.id,
    p.started_ms,
    (extract(epoch from now()) * 1000)::bigint,
    p_retention_days,
    p_dry_run,
    (select n from count_deleted_blocks),
    (select n from count_deleted_txs),
    (select n from count_deleted_metrics),
    (select n from count_deleted_archive),
    'success',
    null
  from params p
  returning *
)
select
  i.id,
  i.started_at,
  i.finished_at,
  i.retention_days,
  i.dry_run,
  i.deleted_blocks,
  i.deleted_txs,
  i.deleted_metrics_daily,
  i.deleted_archive_parts,
  i.status,
  i.error_message
from ins i;
$$;
