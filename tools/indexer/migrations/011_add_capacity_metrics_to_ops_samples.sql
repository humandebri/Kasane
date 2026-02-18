alter table if exists ops_metrics_samples
  add column if not exists estimated_kept_bytes bigint;

alter table if exists ops_metrics_samples
  add column if not exists low_water_bytes bigint;

alter table if exists ops_metrics_samples
  add column if not exists high_water_bytes bigint;

alter table if exists ops_metrics_samples
  add column if not exists hard_emergency_bytes bigint;
