alter table internal_transactions
  add column if not exists trace_sort_key text;

update internal_transactions
set trace_sort_key = (
  select string_agg(lpad(segment, 10, '0'), '_' order by ordinality)
  from unnest(string_to_array(trace_id, '_')) with ordinality as parts(segment, ordinality)
)
where trace_sort_key is null;

alter table internal_transactions
  alter column trace_sort_key set not null;

drop index if exists idx_internal_transactions_from_address;
drop index if exists idx_internal_transactions_to_address;
drop index if exists idx_internal_transactions_created_contract_address;

create index if not exists idx_internal_transactions_from_address
  on internal_transactions(from_address, block_number desc, tx_index desc, trace_sort_key asc);

create index if not exists idx_internal_transactions_to_address
  on internal_transactions(to_address, block_number desc, tx_index desc, trace_sort_key asc);

create index if not exists idx_internal_transactions_created_contract_address
  on internal_transactions(created_contract_address, block_number desc, tx_index desc, trace_sort_key asc);
