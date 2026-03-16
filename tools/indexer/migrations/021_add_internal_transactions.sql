create table if not exists internal_transactions(
  tx_hash bytea not null,
  block_number bigint not null,
  tx_index integer not null,
  trace_id text not null,
  depth integer not null,
  action_type text not null,
  from_address bytea not null,
  to_address bytea,
  created_contract_address bytea,
  value_numeric numeric(78,0) not null,
  success boolean not null,
  error_code text,
  primary key(tx_hash, trace_id)
);

create index if not exists idx_internal_transactions_from_address
  on internal_transactions(from_address, block_number desc, tx_index desc);

create index if not exists idx_internal_transactions_to_address
  on internal_transactions(to_address, block_number desc, tx_index desc);

create index if not exists idx_internal_transactions_created_contract_address
  on internal_transactions(created_contract_address, block_number desc, tx_index desc);
