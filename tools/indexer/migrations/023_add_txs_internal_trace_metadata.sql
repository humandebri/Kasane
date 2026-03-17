alter table txs
  add column if not exists internal_trace_failed boolean not null default false,
  add column if not exists internal_trace_truncated boolean not null default false,
  add column if not exists internal_trace_captured_count integer,
  add column if not exists internal_trace_total_count integer;
