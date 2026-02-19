create index if not exists idx_verify_metrics_samples_sampled_at_desc
  on verify_metrics_samples(sampled_at_ms desc);
