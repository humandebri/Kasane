alter table if exists blocks
  add column if not exists gas_used bigint;
