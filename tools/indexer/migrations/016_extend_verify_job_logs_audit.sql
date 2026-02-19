alter table if exists verify_job_logs
  add column if not exists submitted_by text;

alter table if exists verify_job_logs
  add column if not exists ip_hash text;

alter table if exists verify_job_logs
  add column if not exists ua_hash text;

alter table if exists verify_job_logs
  add column if not exists event_type text;
