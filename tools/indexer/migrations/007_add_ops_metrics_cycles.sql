alter table if exists ops_metrics_samples
  add column if not exists cycles bigint not null default 0;
