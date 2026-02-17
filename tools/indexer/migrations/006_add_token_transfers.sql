create table if not exists token_transfers (
  tx_hash bytea not null references txs(tx_hash) on delete cascade,
  block_number bigint not null,
  tx_index integer not null,
  log_index integer not null,
  token_address bytea not null,
  from_address bytea not null,
  to_address bytea not null,
  amount_numeric numeric(78, 0) not null,
  primary key (tx_hash, log_index)
);

create index if not exists idx_token_transfers_from_block_tx_log_desc
on token_transfers(from_address, block_number desc, tx_index desc, log_index desc);

create index if not exists idx_token_transfers_to_block_tx_log_desc
on token_transfers(to_address, block_number desc, tx_index desc, log_index desc);

create index if not exists idx_token_transfers_token_block_tx_log_desc
on token_transfers(token_address, block_number desc, tx_index desc, log_index desc);
