alter table if exists txs
  add column if not exists receipt_status smallint;

create index if not exists idx_txs_receipt_status_block_tx_desc
on txs(receipt_status, block_number desc, tx_index desc);

create table if not exists ops_metrics_samples (
  sampled_at_ms bigint primary key,
  queue_len bigint not null,
  total_submitted bigint not null,
  total_included bigint not null,
  total_dropped bigint not null,
  drop_counts_json text not null
);

create index if not exists idx_ops_metrics_samples_sampled_at_desc
on ops_metrics_samples(sampled_at_ms desc);
