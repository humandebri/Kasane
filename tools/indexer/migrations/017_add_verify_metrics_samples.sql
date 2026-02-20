create table if not exists verify_metrics_samples (
  sampled_at_ms bigint primary key,
  queue_depth bigint not null,
  success_count bigint not null,
  failed_count bigint not null,
  avg_duration_ms bigint,
  p50_duration_ms bigint,
  p95_duration_ms bigint,
  fail_by_code_json text not null
);
