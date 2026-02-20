create table if not exists tx_receipts_index (
  tx_hash bytea primary key references txs(tx_hash) on delete cascade,
  contract_address bytea,
  status smallint not null,
  block_number bigint not null,
  tx_index integer not null
);

create index if not exists idx_tx_receipts_index_contract_address
  on tx_receipts_index(contract_address);

create index if not exists idx_tx_receipts_index_block_tx
  on tx_receipts_index(block_number desc, tx_index desc);
