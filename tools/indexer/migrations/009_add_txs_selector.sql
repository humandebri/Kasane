alter table if exists txs
  add column if not exists tx_selector bytea;
