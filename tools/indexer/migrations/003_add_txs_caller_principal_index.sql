create index if not exists idx_txs_caller_principal_block_number_tx_index_desc
on txs(caller_principal, block_number desc, tx_index desc);
