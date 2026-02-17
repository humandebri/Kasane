alter table if exists txs
  add column if not exists from_address bytea;

alter table if exists txs
  add column if not exists to_address bytea;

update txs
set from_address = decode(repeat('00', 40), 'hex')
where from_address is null;

alter table if exists txs
  alter column from_address set not null;

create index if not exists idx_txs_from_address_block_tx_desc
on txs(from_address, block_number desc, tx_index desc);

create index if not exists idx_txs_to_address_block_tx_desc
on txs(to_address, block_number desc, tx_index desc);
