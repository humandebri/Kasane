alter table if exists ops_metrics_samples
  add column if not exists pruned_before_block bigint;
